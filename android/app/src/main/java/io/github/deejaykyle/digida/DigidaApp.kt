package io.github.deejaykyle.digida

import android.app.Application
import android.content.Context
import android.content.Intent
import android.os.Environment
import androidx.core.content.ContextCompat
import io.github.deejaykyle.digida.ui.Finish
import io.github.deejaykyle.digida.ui.Mode
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.launch
import uniffi.digida_mobile.Digida
import uniffi.digida_mobile.Listener
import uniffi.digida_mobile.PlayState
import uniffi.digida_mobile.Release
import uniffi.digida_mobile.Song
import uniffi.digida_mobile.Status

/** The process: one Rust core, the library it scanned and the playback status it reports. */
class DigidaApp : Application() {
    lateinit var library: Library
        private set
    lateinit var preferences: Preferences
        private set

    override fun onCreate() {
        super.onCreate()
        preferences = Preferences(this)
        library = Library(this, preferences)
    }
}

/** What the library screens draw from. */
data class Shelf(
    val scanning: Boolean = false,
    val albums: List<Release> = emptyList(),
    val songs: List<Song> = emptyList(),
    val error: String? = null,
)

/**
 * The Rust core and the state the screens observe. Library calls block in Rust, so they run on
 * the IO dispatcher; playback calls return at once and report back through [status].
 */
class Library(private val context: Context, private val preferences: Preferences) {
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.IO)
    private val core = Digida(
        context.filesDir.resolve("core").path,
        context.cacheDir.resolve("core").path,
    )

    private val _shelf = MutableStateFlow(Shelf())
    val shelf: StateFlow<Shelf> = _shelf.asStateFlow()

    private val _status = MutableStateFlow(core.status())
    val status: StateFlow<Status> = _status.asStateFlow()

    init {
        core.setListener(object : Listener {
            override fun changed(status: Status) {
                val was = _status.value.state
                _status.value = status
                if (was == PlayState.IDLE && status.state != PlayState.IDLE) {
                    PlaybackService.start(context)
                }
            }
        })
    }

    /** Scans the chosen folders, or the phone's Music and Download folders when none are. */
    fun scan() {
        if (_shelf.value.scanning) return
        _shelf.value = _shelf.value.copy(scanning = true, error = null)
        scope.launch {
            _shelf.value = try {
                core.scan(preferences.folders.ifEmpty { defaultFolders() })
                Shelf(albums = core.albums(), songs = core.songs())
            } catch (error: Exception) {
                _shelf.value.copy(scanning = false, error = error.message)
            }
        }
    }

    /** The songs of one album, in order. */
    suspend fun albumSongs(album: Release): List<Song> =
        kotlinx.coroutines.withContext(Dispatchers.IO) { core.albumSongs(album.id) }

    fun play(songs: List<Song>, start: Int) = core.play(songs, start.toUInt())
    fun toggle() = core.toggle()
    fun next() = core.next()
    fun previous() = core.previous()
    fun seek(positionMs: Long) = core.seek(positionMs.coerceAtLeast(0).toULong())

    private fun defaultFolders(): List<String> = listOf(
        Environment.DIRECTORY_MUSIC,
        Environment.DIRECTORY_DOWNLOADS,
    ).map { Environment.getExternalStoragePublicDirectory(it).path }
}

/** The choices the app keeps between launches. */
class Preferences(context: Context) {
    private val store = context.getSharedPreferences("digida", Context.MODE_PRIVATE)

    private val _finish = MutableStateFlow(Finish.from(store.getString("finish", null)))
    val finish: StateFlow<Finish> = _finish.asStateFlow()

    private val _mode = MutableStateFlow(Mode.from(store.getString("mode", null)))
    val mode: StateFlow<Mode> = _mode.asStateFlow()

    private val _folders = MutableStateFlow(store.getStringSet("folders", emptySet())!!.sorted())
    val foldersFlow: StateFlow<List<String>> = _folders.asStateFlow()
    val folders: List<String> get() = _folders.value

    fun setFinish(finish: Finish) {
        _finish.value = finish
        store.edit().putString("finish", finish.id).apply()
    }

    fun setMode(mode: Mode) {
        _mode.value = mode
        store.edit().putString("mode", mode.id).apply()
    }

    fun setFolders(folders: List<String>) {
        _folders.value = folders.distinct().sorted()
        store.edit().putStringSet("folders", folders.toSet()).apply()
    }
}

/** Starts a service the way Android 8 and later want one started while the app is in front. */
fun Context.startForegroundServiceCompat(intent: Intent) =
    ContextCompat.startForegroundService(this, intent)
