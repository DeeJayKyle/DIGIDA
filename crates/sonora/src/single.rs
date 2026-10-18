use std::io::{BufRead, BufReader, ErrorKind, Read, Write};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc::{SyncSender, sync_channel};
use std::thread;
use std::time::{Duration, Instant};

use interprocess::local_socket::traits::{ListenerExt as _, Stream as _};
use interprocess::local_socket::{
    GenericNamespaced, Listener, ListenerOptions, Name, Stream, ToNsName,
};
use tokio::sync::mpsc::UnboundedSender;

const SOCKET: &str = match cfg!(debug_assertions) {
    true => "sonora-dev.sock",
    false => "sonora.sock",
};
const MAX_CONNECTIONS: usize = 16;
const MAX_URI_BYTES: usize = 4 * 1024;
const MAX_OPEN_BYTES: usize = 64 * 1024;
const IO_TIMEOUT: Duration = Duration::from_secs(2);
const REPLY_TIMEOUT: Duration = Duration::from_secs(2);
const POLL_INTERVAL: Duration = Duration::from_millis(5);

pub enum Instance {
    First,
    Running,
    Failed,
}

/// A message received from another Sonora invocation or a client connected to the instance
/// socket.
pub enum Message {
    Open(Vec<String>),
    Api(ApiRequest),
}

/// An incoming `sonora-api://` command paired with an atomic claim flag so a request that times
/// out waiting for the main thread is cancelled and never executes late.
pub struct ApiRequest {
    uri: String,
    reply: Option<SyncSender<String>>,
    claimed: Arc<AtomicBool>,
}

impl ApiRequest {
    pub fn detached(uri: String) -> Self {
        Self {
            uri,
            reply: None,
            claimed: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Claims this request for execution. Returns `None` if the caller already timed out and
    /// cancelled the request.
    pub fn take(&self) -> Option<&str> {
        match self.claimed.swap(true, Ordering::AcqRel) {
            true => None,
            false => Some(&self.uri),
        }
    }

    pub fn respond(self, response: String) {
        if let Some(reply) = self.reply {
            let _ = reply.send(response);
        }
    }
}

struct ConnectionGuard(Arc<AtomicUsize>);

impl ConnectionGuard {
    fn try_acquire(active: &Arc<AtomicUsize>) -> Option<Self> {
        active
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |count| {
                (count < MAX_CONNECTIONS).then_some(count + 1)
            })
            .ok()
            .map(|_| Self(Arc::clone(active)))
    }
}

impl Drop for ConnectionGuard {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::AcqRel);
    }
}

struct TimedStream {
    inner: Stream,
    deadline: Instant,
}

impl TimedStream {
    fn new(inner: Stream, timeout: Duration) -> std::io::Result<Self> {
        #[cfg(unix)]
        {
            inner.set_recv_timeout(Some(timeout))?;
            inner.set_send_timeout(Some(timeout))?;
        }
        Ok(Self {
            inner,
            deadline: Instant::now() + timeout,
        })
    }

    fn reset_deadline(&mut self, timeout: Duration) {
        self.deadline = Instant::now() + timeout;
        #[cfg(unix)]
        {
            let _ = self.inner.set_recv_timeout(Some(timeout));
            let _ = self.inner.set_send_timeout(Some(timeout));
        }
    }
}

impl Read for TimedStream {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        #[cfg(windows)]
        while !pipe_ready_or_closed(&self.inner) {
            if Instant::now() >= self.deadline {
                return Err(std::io::Error::new(
                    ErrorKind::TimedOut,
                    "socket read timed out",
                ));
            }
            thread::sleep(POLL_INTERVAL);
        }

        loop {
            match self.inner.read(buf) {
                Err(error) if error.kind() == ErrorKind::WouldBlock => {
                    if Instant::now() >= self.deadline {
                        return Err(std::io::Error::new(
                            ErrorKind::TimedOut,
                            "socket read timed out",
                        ));
                    }
                    thread::sleep(POLL_INTERVAL);
                }
                Err(error) if error.kind() == ErrorKind::Interrupted => continue,
                other => return other,
            }
        }
    }
}

impl Write for TimedStream {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        loop {
            match self.inner.write(buf) {
                Err(error) if error.kind() == ErrorKind::WouldBlock => {
                    if Instant::now() >= self.deadline {
                        return Err(std::io::Error::new(
                            ErrorKind::TimedOut,
                            "socket write timed out",
                        ));
                    }
                    thread::sleep(POLL_INTERVAL);
                }
                Err(error) if error.kind() == ErrorKind::Interrupted => continue,
                other => return other,
            }
        }
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
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
        return match request(name, uri, IO_TIMEOUT) {
            true => Instance::Running,
            false => Instance::Failed,
        };
    }

    let listener = match options(name.clone()).create_sync() {
        Ok(listener) => listener,

        Err(error) => {
            // Windows reports an occupied pipe with an error other than
            // AddrInUse, so any failure may mean another Sonora owns the socket.
            if hand_over(name.clone(), args, IO_TIMEOUT) {
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

    spawn_listener(listener, sender, IO_TIMEOUT, REPLY_TIMEOUT);
    Instance::First
}

fn spawn_listener(
    listener: Listener,
    sender: UnboundedSender<Message>,
    io_timeout: Duration,
    reply_timeout: Duration,
) {
    let active = Arc::new(AtomicUsize::new(0));
    thread::spawn(move || {
        for connection in listener.incoming() {
            let Ok(connection) = connection else {
                continue;
            };
            let Some(guard) = ConnectionGuard::try_acquire(&active) else {
                log::warn!("single: dropping socket connection at concurrency limit");
                continue;
            };
            let sender = sender.clone();
            thread::spawn(move || {
                let _guard = guard;
                serve(connection, sender, io_timeout, reply_timeout);
            });
        }
    });
}

fn serve(
    connection: Stream,
    sender: UnboundedSender<Message>,
    io_timeout: Duration,
    reply_timeout: Duration,
) {
    let Ok(stream) = TimedStream::new(connection, io_timeout) else {
        return;
    };
    let mut reader = BufReader::new(stream);
    let Ok(first) = read_bounded_line(&mut reader, MAX_OPEN_BYTES) else {
        return;
    };
    if first.trim().is_empty() {
        let _ = sender.send(Message::Open(Vec::new()));
        return;
    }

    match crate::api::is_uri(&first) {
        true => {
            let mut line = first;
            loop {
                if line.len() > MAX_URI_BYTES {
                    reader.get_mut().reset_deadline(io_timeout);
                    let _ = write_line(reader.get_mut(), &crate::api::uri_too_long());
                    break;
                }
                let trimmed = line.trim();
                if crate::api::is_uri(trimmed) {
                    let (tx, rx) = sync_channel(1);
                    let claimed = Arc::new(AtomicBool::new(false));
                    if sender
                        .send(Message::Api(ApiRequest {
                            uri: trimmed.to_owned(),
                            reply: Some(tx),
                            claimed: Arc::clone(&claimed),
                        }))
                        .is_err()
                    {
                        break;
                    }
                    let response = match rx.recv_timeout(reply_timeout) {
                        Ok(response) => response,
                        Err(_) => match claimed.swap(true, Ordering::AcqRel) {
                            false => crate::api::timed_out(),
                            true => rx
                                .recv_timeout(Duration::from_millis(100))
                                .unwrap_or_else(|_| crate::api::timed_out()),
                        },
                    };
                    reader.get_mut().reset_deadline(io_timeout);
                    if write_line(reader.get_mut(), &response).is_err() {
                        break;
                    }
                }
                reader.get_mut().reset_deadline(io_timeout);
                match read_bounded_line(&mut reader, MAX_URI_BYTES) {
                    Ok(next) if next.is_empty() => break,
                    Ok(next) => line = next,
                    Err(error) if error.kind() == ErrorKind::InvalidData => {
                        reader.get_mut().reset_deadline(io_timeout);
                        let _ = write_line(reader.get_mut(), &crate::api::uri_too_long());
                        break;
                    }
                    Err(_) => break,
                }
            }
        }
        false => {
            let remaining = MAX_OPEN_BYTES.saturating_sub(first.len());
            let mut rest = String::new();
            if (&mut reader)
                .take((remaining + 1) as u64)
                .read_to_string(&mut rest)
                .is_err()
                || rest.len() > remaining
            {
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

fn read_bounded_line(reader: &mut impl BufRead, limit: usize) -> std::io::Result<String> {
    let mut buf = Vec::new();
    let count = reader
        .take((limit + 1) as u64)
        .read_until(b'\n', &mut buf)?;
    if count > limit {
        return Err(std::io::Error::new(
            ErrorKind::InvalidData,
            "line exceeds maximum length",
        ));
    }
    String::from_utf8(buf).map_err(|error| std::io::Error::new(ErrorKind::InvalidData, error))
}

fn write_line(stream: &mut impl Write, line: &str) -> std::io::Result<()> {
    stream.write_all(line.as_bytes())?;
    stream.write_all(b"\n")?;
    stream.flush()
}

fn request(name: Name<'_>, uri: &str, timeout: Duration) -> bool {
    let Some(response) = send_request(name, uri, timeout) else {
        emit_json(&crate::api::not_running());
        return false;
    };
    emit_json(&response);
    crate::api::is_ok(&response)
}

fn send_request(name: Name<'_>, uri: &str, timeout: Duration) -> Option<String> {
    if uri.len() > MAX_URI_BYTES {
        return Some(crate::api::uri_too_long());
    }
    let stream = Stream::connect(name).ok()?;
    let mut stream = TimedStream::new(stream, timeout).ok()?;
    write_line(&mut stream, uri).ok()?;
    stream.reset_deadline(timeout);
    let mut reader = BufReader::new(stream);
    let response = read_bounded_line(&mut reader, MAX_OPEN_BYTES).ok()?;
    match response.trim().is_empty() {
        true => None,
        false => Some(response.trim().to_owned()),
    }
}

fn emit_json(line: &str) {
    #[cfg(windows)]
    attach_parent_console();

    let mut stdout = std::io::stdout().lock();
    let _ = writeln!(stdout, "{}", line.trim());
    let _ = stdout.flush();
}

/// Release builds on Windows use `windows_subsystem = "windows"`, so an interactive launch from
/// `cmd.exe` or PowerShell without stdout redirection starts with a null `STD_OUTPUT_HANDLE`.
/// Attaching to the parent console only when stdout is missing preserves redirected pipes while
/// making interactive terminal calls print their JSON reply.
#[cfg(windows)]
fn attach_parent_console() {
    use std::os::windows::io::AsRawHandle as _;

    const ATTACH_PARENT_PROCESS: u32 = u32::MAX;
    const INVALID_HANDLE: *mut std::ffi::c_void = -1isize as *mut std::ffi::c_void;

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn AttachConsole(dwProcessId: u32) -> i32;
    }

    let handle = std::io::stdout().as_raw_handle();
    if handle.is_null() || handle == INVALID_HANDLE {
        // SAFETY: `AttachConsole(ATTACH_PARENT_PROCESS)` is a thread-safe Win32 call with no
        // pointer or lifetime preconditions; failure (e.g. when launched from Win+R without a
        // parent console) is harmless and ignored.
        unsafe {
            AttachConsole(ATTACH_PARENT_PROCESS);
        }
    }
}

/// Checks whether a Windows named pipe has bytes ready to read or has been closed by the peer,
/// without blocking the calling thread.
#[cfg(windows)]
fn pipe_ready_or_closed(stream: &Stream) -> bool {
    use std::os::windows::io::{AsHandle as _, AsRawHandle as _};

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn PeekNamedPipe(
            hNamedPipe: *mut std::ffi::c_void,
            lpBuffer: *mut std::ffi::c_void,
            nBufferSize: u32,
            lpBytesRead: *mut u32,
            lpTotalBytesAvail: *mut u32,
            lpBytesLeftThisMessage: *mut u32,
        ) -> i32;
    }

    let Stream::NamedPipe(pipe) = stream;
    let handle = pipe.as_handle().as_raw_handle();
    let mut available: u32 = 0;
    // SAFETY: `handle` is a live pipe handle owned by `stream`, and all buffer pointers are null
    // with a zero-length buffer while querying `available`.
    let status = unsafe {
        PeekNamedPipe(
            handle,
            std::ptr::null_mut(),
            0,
            std::ptr::null_mut(),
            &mut available,
            std::ptr::null_mut(),
        )
    };
    status == 0 || available > 0
}

fn hand_over(name: Name<'_>, args: &[String], timeout: Duration) -> bool {
    let stream = match Stream::connect(name) {
        Ok(stream) => stream,
        Err(error) => {
            log::warn!("single: cannot reach the running instance: {error:#}");
            return false;
        }
    };
    let Ok(mut stream) = TimedStream::new(stream, timeout) else {
        return false;
    };
    write_line(&mut stream, &args.join("\n")).is_ok()
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

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::sync::mpsc::unbounded_channel;

    static NEXT_SOCKET_ID: AtomicUsize = AtomicUsize::new(1);

    fn unique_socket_name() -> String {
        let id = NEXT_SOCKET_ID.fetch_add(1, Ordering::Relaxed);
        format!("sonora-test-{}-{id}.sock", std::process::id())
    }

    #[test]
    fn empty_hand_over_sends_empty_open_to_restore_window() {
        let raw_name = unique_socket_name();
        let name = raw_name.as_str().to_ns_name::<GenericNamespaced>().unwrap();
        let listener = options(name.clone()).create_sync().unwrap();
        let (tx, mut rx) = unbounded_channel();
        spawn_listener(listener, tx, IO_TIMEOUT, REPLY_TIMEOUT);

        assert!(hand_over(name, &[], IO_TIMEOUT));
        match rx.blocking_recv() {
            Some(Message::Open(items)) => assert!(items.is_empty()),
            _ => panic!("expected empty Message::Open"),
        }
    }

    #[test]
    fn multi_file_hand_over_delivers_all_paths() {
        let raw_name = unique_socket_name();
        let name = raw_name.as_str().to_ns_name::<GenericNamespaced>().unwrap();
        let listener = options(name.clone()).create_sync().unwrap();
        let (tx, mut rx) = unbounded_channel();
        spawn_listener(listener, tx, IO_TIMEOUT, REPLY_TIMEOUT);

        let args = vec!["/music/one.flac".to_owned(), "/music/two.mp3".to_owned()];
        assert!(hand_over(name, &args, IO_TIMEOUT));
        match rx.blocking_recv() {
            Some(Message::Open(items)) => assert_eq!(items, args),
            _ => panic!("expected Message::Open with two files"),
        }
    }

    #[test]
    fn api_request_roundtrip_and_error_status() {
        let raw_name = unique_socket_name();
        let name = raw_name.as_str().to_ns_name::<GenericNamespaced>().unwrap();
        let listener = options(name.clone()).create_sync().unwrap();
        let (tx, mut rx) = unbounded_channel();
        spawn_listener(listener, tx, IO_TIMEOUT, REPLY_TIMEOUT);

        thread::spawn(move || {
            while let Some(Message::Api(req)) = rx.blocking_recv() {
                if let Some(uri) = req.take() {
                    let reply = match uri.ends_with("current") {
                        true => r#"{"ok":true,"data":{"state":"playing"}}"#.to_owned(),
                        false => r#"{"ok":false,"error":"unknown endpoint: bad"}"#.to_owned(),
                    };
                    req.respond(reply);
                }
            }
        });

        let ok_reply = send_request(name.clone(), "sonora-api://current", IO_TIMEOUT).unwrap();
        assert!(crate::api::is_ok(&ok_reply));

        let err_reply = send_request(name, "sonora-api://bad", IO_TIMEOUT).unwrap();
        assert!(!crate::api::is_ok(&err_reply));
    }

    #[test]
    fn timed_out_api_request_cannot_execute_late() {
        let raw_name = unique_socket_name();
        let name = raw_name.as_str().to_ns_name::<GenericNamespaced>().unwrap();
        let listener = options(name.clone()).create_sync().unwrap();
        let (tx, mut rx) = unbounded_channel();
        spawn_listener(listener, tx, IO_TIMEOUT, Duration::from_millis(40));

        let reply = send_request(name, "sonora-api://toggle", IO_TIMEOUT).unwrap();
        assert_eq!(reply, crate::api::timed_out());
        assert!(!crate::api::is_ok(&reply));

        let Some(Message::Api(req)) = rx.blocking_recv() else {
            panic!("expected queued Message::Api");
        };
        assert_eq!(req.take(), None);
    }

    #[test]
    fn rejects_oversized_api_and_open_payloads() {
        let raw_name = unique_socket_name();
        let name = raw_name.as_str().to_ns_name::<GenericNamespaced>().unwrap();
        let listener = options(name.clone()).create_sync().unwrap();
        let (tx, mut rx) = unbounded_channel();
        spawn_listener(listener, tx, IO_TIMEOUT, REPLY_TIMEOUT);

        let huge_uri = format!("sonora-api://{}", "a".repeat(MAX_URI_BYTES + 128));
        let mut stream = Stream::connect(name.clone()).unwrap();
        write_line(&mut stream, &huge_uri).unwrap();
        let mut reader = BufReader::new(stream);
        let reply = read_bounded_line(&mut reader, MAX_OPEN_BYTES).unwrap();
        assert_eq!(reply.trim(), crate::api::uri_too_long());
        assert!(rx.try_recv().is_err());

        let huge_open = vec!["x".repeat(MAX_OPEN_BYTES + 1024)];
        let _ = hand_over(name.clone(), &huge_open, IO_TIMEOUT);
        let probe = vec!["/music/valid.mp3".to_owned()];
        assert!(hand_over(name, &probe, IO_TIMEOUT));
        match rx.blocking_recv() {
            Some(Message::Open(items)) => assert_eq!(items, probe),
            _ => panic!("expected valid probe after oversized payload was dropped"),
        }
    }

    #[test]
    fn enforces_connection_concurrency_cap() {
        let active = Arc::new(AtomicUsize::new(0));
        let mut guards: Vec<ConnectionGuard> = (0..MAX_CONNECTIONS)
            .map(|_| ConnectionGuard::try_acquire(&active).expect("slot should be available"))
            .collect();

        assert!(ConnectionGuard::try_acquire(&active).is_none());
        guards.pop();
        assert!(ConnectionGuard::try_acquire(&active).is_some());
    }

    #[test]
    fn idle_connection_times_out_without_blocking_listener() {
        let raw_name = unique_socket_name();
        let name = raw_name.as_str().to_ns_name::<GenericNamespaced>().unwrap();
        let listener = options(name.clone()).create_sync().unwrap();
        let (tx, mut rx) = unbounded_channel();
        spawn_listener(
            listener,
            tx,
            Duration::from_millis(40),
            Duration::from_millis(40),
        );

        let _stalled = Stream::connect(name.clone()).unwrap();
        thread::sleep(Duration::from_millis(80));
        assert!(rx.try_recv().is_err());

        let probe = vec!["/music/after-timeout.flac".to_owned()];
        assert!(hand_over(name, &probe, IO_TIMEOUT));
        match rx.blocking_recv() {
            Some(Message::Open(items)) => assert_eq!(items, probe),
            _ => panic!("expected Message::Open after stalled connection timed out"),
        }
    }
}
