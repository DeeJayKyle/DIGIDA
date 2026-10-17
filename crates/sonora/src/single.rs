use std::io::{BufRead, BufReader, ErrorKind, Read, Write};
use std::sync::mpsc::{SyncSender, sync_channel};
use std::thread;
use std::time::Duration;

use interprocess::local_socket::traits::{ListenerExt as _, Stream as _};
use interprocess::local_socket::{GenericNamespaced, ListenerOptions, Name, Stream, ToNsName};
use tokio::sync::mpsc::UnboundedSender;

const SOCKET: &str = match cfg!(debug_assertions) {
    true => "sonora-dev.sock",
    false => "sonora.sock",
};
const REPLY_TIMEOUT: Duration = Duration::from_secs(2);

pub enum Instance {
    First,
    Running,
    Failed,
}

/// A message received from another Sonora invocation or a client connected to the instance
/// socket. An `Api` request carries a channel when the caller is waiting on a JSON reply.
pub enum Message {
    Open(Vec<String>),
    Api {
        uri: String,
        reply: Option<SyncSender<String>>,
    },
}

/// `args` is every non-flag command-line argument of this launch: a `sonora-api://` command, a
/// lone `spotify:`/web link, or one or more file paths from an "Open With" selection.
pub fn claim(args: &[String], sender: UnboundedSender<Message>) -> Instance {
    let name = match SOCKET.to_ns_name::<GenericNamespaced>() {
        Ok(name) => name,
        Err(error) => {
            log::warn!("single: cannot name the instance socket: {error:#}");
            return Instance::Failed;
        }
    };

    if let Some(uri) = crate::api::extract(args) {
        request(name, uri);
        return Instance::Running;
    }

    let listener = match options(name.clone()).create_sync() {
        Ok(listener) => listener,

        Err(error) => {
            // Windows reports an occupied pipe with an error other than
            // AddrInUse, so any failure may mean another Sonora owns the socket.
            if hand_over(name.clone(), args) {
                return Instance::Running;
            }

            // Only a filesystem socket can leave a stale AddrInUse behind.
            if error.kind() != ErrorKind::AddrInUse {
                log::error!("single: cannot own the instance socket: {error:#}");
                return Instance::Failed;
            }

            log::warn!("single: socket is occupied but unreachable; attempting recovery");

            match options(name.clone())
                .try_overwrite(true)
                .max_spin_time(Duration::ZERO)
                .create_sync()
            {
                Ok(listener) => listener,
                Err(error) => {
                    log::error!("single: cannot recover instance socket: {error:#}");
                    return Instance::Failed;
                }
            }
        }
    };

    thread::spawn(move || {
        for connection in listener.incoming() {
            let Ok(connection) = connection else {
                continue;
            };
            let sender = sender.clone();
            thread::spawn(move || serve(connection, sender));
        }
    });

    Instance::First
}

fn serve(connection: Stream, sender: UnboundedSender<Message>) {
    let mut reader = BufReader::new(connection);
    let mut first = String::new();
    if reader.read_line(&mut first).is_err() || first.is_empty() {
        return;
    }

    match crate::api::is_uri(&first) {
        true => {
            let mut line = first;
            loop {
                let trimmed = line.trim();
                if crate::api::is_uri(trimmed) {
                    let (tx, rx) = sync_channel(1);
                    if sender
                        .send(Message::Api {
                            uri: trimmed.to_owned(),
                            reply: Some(tx),
                        })
                        .is_err()
                    {
                        break;
                    }
                    let response = rx
                        .recv_timeout(REPLY_TIMEOUT)
                        .unwrap_or_else(|_| crate::api::not_running());
                    let stream = reader.get_mut();
                    if stream.write_all(response.as_bytes()).is_err()
                        || stream.write_all(b"\n").is_err()
                        || stream.flush().is_err()
                    {
                        break;
                    }
                }
                line.clear();
                match reader.read_line(&mut line) {
                    Ok(0) | Err(_) => break,
                    Ok(_) => {}
                }
            }
        }
        false => {
            let mut rest = String::new();
            if reader.read_to_string(&mut rest).is_err() {
                return;
            }
            let items: Vec<String> = first
                .lines()
                .chain(rest.lines())
                .map(str::trim)
                .filter(|item| !item.is_empty())
                .map(str::to_owned)
                .collect();
            let _ = sender.send(Message::Open(items));
        }
    }
}

fn request(name: Name<'_>, uri: &str) {
    let mut stream = match Stream::connect(name) {
        Ok(stream) => stream,
        Err(_) => {
            println!("{}", crate::api::not_running());
            return;
        }
    };
    if stream.write_all(uri.as_bytes()).is_err()
        || stream.write_all(b"\n").is_err()
        || stream.flush().is_err()
    {
        println!("{}", crate::api::not_running());
        return;
    }
    let mut reader = BufReader::new(stream);
    let mut response = String::new();
    match reader.read_line(&mut response) {
        Ok(count) if count > 0 => println!("{}", response.trim()),
        _ => println!("{}", crate::api::not_running()),
    }
}

fn hand_over(name: Name<'_>, args: &[String]) -> bool {
    let mut stream = match Stream::connect(name) {
        Ok(stream) => stream,
        Err(error) => {
            log::warn!("single: cannot reach the running instance: {error:#}");
            return false;
        }
    };
    stream.write_all(args.join("\n").as_bytes()).is_ok()
}

/// The listener for the instance socket, open to every Sonora launched at the desktop.
fn options(name: Name<'_>) -> ListenerOptions<'_> {
    let options = ListenerOptions::new().name(name);
    #[cfg(windows)]
    let options = match reachable() {
        Some(descriptor) => {
            use interprocess::os::windows::local_socket::ListenerOptionsExt as _;
            options.security_descriptor(descriptor)
        }
        None => options,
    };
    options
}

/// Access to the pipe for anyone at the desktop, whichever token they hold. Windows owns an
/// elevated process's objects as Administrators and denies that group to the same user's
/// unelevated processes, so with the default descriptor a Sonora the updater relaunched
/// elevated is out of reach for one launched from the taskbar: the hand-off fails and the second
/// launch exits without ever showing a window. `None` keeps the default, and is logged.
#[cfg(windows)]
fn reachable() -> Option<interprocess::os::windows::security_descriptor::SecurityDescriptor> {
    use interprocess::os::windows::security_descriptor::SecurityDescriptor;

    match SecurityDescriptor::deserialize(widestring::u16cstr!("D:(A;;GA;;;IU)")) {
        Ok(descriptor) => Some(descriptor),
        Err(error) => {
            log::warn!("single: cannot describe the socket's access: {error:#}");
            None
        }
    }
}
