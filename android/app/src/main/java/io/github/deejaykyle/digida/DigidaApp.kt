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
import kotlinx.coroutines.withContext
import uniffi.digida_mobile.ArtistPage
import uniffi.digida_mobile.Digida
import uniffi.digida_mobile.Feed
import uniffi.digida_mobile.Words
import uniffi.digida_mobile.Listener
import uniffi.digida_mobile.Method
import uniffi.digida_mobile.PlayState
import uniffi.digida_mobile.Playlist
import uniffi.digida_mobile.Prompt
import uniffi.digida_mobile.Release
import uniffi.digida_mobile.Service
import uniffi.digida_mobile.Song
import uniffi.digida_mobile.Status
import uniffi.digida_mobile.WebLogin

/** The process: one Rust core, the library it shows and the playback status it reports. */
class DigidaApp : Application() {
    lateinit var library: Library
        private set
    lateinit var preferences: Preferences
        private set

    override fun onCreate() {
        super.onCreate()
        Tls.init(this)
        preferences = Preferences(this)
        library = Library(this, preferences)
    }
}

/** The core's HTTPS certificate checks, which read the phone's trust store through the app. */
object Tls {
    init {
        System.loadLibrary("digida_mobile")
    }

    external fun init(context: Context)
}

/** What the library screens draw from, for the service they show. */
data class Shelf(
    val loading: Boolean = false,
    val albums: List<Release> = emptyList(),
    val songs: List<Song> = emptyList(),
    val playlists: List<Playlist> = emptyList(),
    val error: String? = null,
    /** The service is signed in as a guest, which has no library to list, only search. */
    val guest: Boolean = false,
)

/** A sign-in waiting on the listener: which service asked, and what it asked for. */
data class Asking(val service: String, val prompt: Prompt)

/**
 * The Rust core and the state the screens observe. Library calls block in Rust, so they run on
 * the IO dispatcher; playback calls return at once and report back through [status].
 */
class Library(private val context: Context, private val preferences: Preferences) {
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.IO)
    private val core = Digida(
        context.filesDir.resolve("core").path,
        context.cacheDir.resolve("core").path,
        preferences.normalise.value,
    ).also { it.setAutoplay(preferences.autoplay.value) }

    /** The sleep timer as set: minutes from now, or the end of the song, or neither. */
    private val _sleep = MutableStateFlow<Sleep>(Sleep.Off)
    val sleep: StateFlow<Sleep> = _sleep.asStateFlow()

    private val _shelf = MutableStateFlow(Shelf())
    val shelf: StateFlow<Shelf> = _shelf.asStateFlow()

    private val _status = MutableStateFlow(core.status())
    val status: StateFlow<Status> = _status.asStateFlow()

    private val _services = MutableStateFlow(core.services())
    val services: StateFlow<List<Service>> = _services.asStateFlow()

    private val _active = MutableStateFlow(LOCAL)
    val active: StateFlow<String> = _active.asStateFlow()

    private val _asking = MutableStateFlow<Asking?>(null)
    val asking: StateFlow<Asking?> = _asking.asStateFlow()

    /** The service a sign-in is running for, while one is. */
    private val _signing = MutableStateFlow<String?>(null)
    val signing: StateFlow<String?> = _signing.asStateFlow()

    private val _problem = MutableStateFlow<String?>(null)
    val problem: StateFlow<String?> = _problem.asStateFlow()

    init {
        core.setListener(object : Listener {
            override fun changed(status: Status) {
                val was = _status.value.state
                _status.value = status
                if (was == PlayState.IDLE && status.state != PlayState.IDLE) {
                    PlaybackService.start(context)
                }
            }

            override fun prompt(provider: String, prompt: Prompt) {
                _asking.value = Asking(provider, prompt)
            }
        })
        scope.launch {
            core.restore()
            _services.value = core.services()
        }
    }

    /** Scans the chosen folders, or the phone's Music and Download folders when none are. */
    fun scan() {
        scope.launch { rescan() }
    }

    /** Scans the local folders, and refreshes the screen only while local files are on it. */
    private fun rescan() {
        if (_active.value == LOCAL) _shelf.value = _shelf.value.copy(loading = true, error = null)
        val failed = runCatching { core.scan(preferences.folders.ifEmpty { defaultFolders() }) }
            .exceptionOrNull()
        _services.value = core.services()
        if (_active.value != LOCAL) return
        when (failed) {
            null -> load(LOCAL)
            else -> _shelf.value = _shelf.value.copy(loading = false, error = failed.message)
        }
    }

    /** Shows `slug`'s library on the library screens and loads it. */
    fun show(slug: String, reload: Boolean = false) {
        if (!reload && _active.value == slug && _shelf.value.error == null && !_shelf.value.loading) return
        _active.value = slug
        core.setActive(slug)
        val guest = slug != LOCAL && _services.value.any { it.slug == slug && it.signedIn && !it.authenticated }
        if (guest) {
            _shelf.value = Shelf(guest = true)
            return
        }
        _shelf.value = Shelf(loading = true)
        scope.launch { load(slug) }
    }

    /**
     * Loads `slug`'s albums, songs and playlists, showing them only if `slug` is still the
     * library on screen once they arrive. A part that fails is left empty.
     */
    private fun load(slug: String) {
        val albums = runCatching { core.albums() }
        val songs = runCatching { core.songs() }
        val playlists = runCatching { core.playlists() }
        if (_active.value != slug) return
        _shelf.value = Shelf(
            albums = albums.getOrDefault(emptyList()),
            songs = songs.getOrDefault(emptyList()),
            playlists = playlists.getOrDefault(emptyList()),
            error = listOf(albums, songs).firstNotNullOfOrNull { it.exceptionOrNull()?.message },
        )
    }

    /**
     * Signs in to `slug`. Prompts arrive on [asking]; once it is done the app comes back to the
     * front, closing the browser tab the sign-in may have opened, and shows that library.
     */
    fun signIn(slug: String, method: Method) {
        if (_signing.value != null) return
        _signing.value = slug
        _problem.value = null
        scope.launch {
            val failed = runCatching { core.signIn(slug, method) }.exceptionOrNull()
            _asking.value = null
            _signing.value = null
            _services.value = core.services()
            if (failed != null) {
                _problem.value = failed.message
            } else {
                context.startActivity(
                    Intent(context, MainActivity::class.java)
                        .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_CLEAR_TOP)
                )
                show(slug, reload = true)
            }
        }
    }

    /** Answers the prompt the running sign-in raised. */
    fun answer(text: String) {
        _asking.value = null
        core.answer(text)
    }

    fun cancelSignIn() {
        val slug = _signing.value ?: return
        _asking.value = null
        core.cancelSignIn(slug)
    }

    fun signOut(slug: String) {
        core.signOut(slug)
        _services.value = core.services()
        if (_active.value == slug) show(LOCAL)
    }

    /** How `slug` signs in through a web view, for a provider that takes cookies. */
    fun webLogin(slug: String): WebLogin? = core.webLogin(slug)

    fun dismissProblem() {
        _problem.value = null
    }

    suspend fun albumSongs(album: Release): List<Song> = withContext(Dispatchers.IO) { core.albumSongs(album.id) }
    suspend fun playlistSongs(playlist: Playlist): List<Song> =
        withContext(Dispatchers.IO) { core.playlistSongs(playlist.id) }
    suspend fun search(query: String): List<Song> = withContext(Dispatchers.IO) { core.search(query) }

    suspend fun home(): Feed = withContext(Dispatchers.IO) { core.home() }
    suspend fun artist(id: String): ArtistPage = withContext(Dispatchers.IO) { core.artist(id) }
    suspend fun lyrics(song: Song): Words? = withContext(Dispatchers.IO) { core.lyrics(song) }
    suspend fun queue(): List<Song> = withContext(Dispatchers.IO) { core.queue() }

    /** Hearts a song, or takes the heart away, reporting a failure along the bottom. */
    fun setLiked(song: Song, liked: Boolean) {
        scope.launch {
            runCatching { core.setLiked(song, liked) }.onFailure { _problem.value = it.message }
        }
    }

    /** Plays a station of songs like `song`, reporting a provider that has none. */
    fun startRadio(song: Song) {
        scope.launch {
            runCatching { core.startRadio(song) }.onFailure { _problem.value = it.message }
        }
    }

    fun setAutoplay(on: Boolean) {
        preferences.setAutoplay(on)
        core.setAutoplay(on)
    }

    fun setSleep(sleep: Sleep) {
        _sleep.value = sleep
        when (sleep) {
            Sleep.Off -> core.setSleepTimer(0u, false)
            Sleep.EndOfSong -> core.setSleepTimer(0u, true)
            is Sleep.Minutes -> core.setSleepTimer(sleep.minutes.toUInt(), false)
        }
    }

    fun playNext(song: Song) = core.playNext(song)
    fun addToQueue(song: Song) = core.addToQueue(song)
    fun removeFromQueue(index: Int) = core.removeFromQueue(index.toUInt())
    fun jump(index: Int) = core.jump(index.toUInt())

    fun play(songs: List<Song>, start: Int) = core.play(songs, start.toUInt())
    fun toggle() = core.toggle()
    fun next() = core.next()
    fun previous() = core.previous()
    fun seek(positionMs: Long) = core.seek(positionMs.coerceAtLeast(0).toULong())

    private fun defaultFolders(): List<String> = listOf(
        Environment.DIRECTORY_MUSIC,
        Environment.DIRECTORY_DOWNLOADS,
    ).map { Environment.getExternalStoragePublicDirectory(it).path }

    companion object {
        const val LOCAL = "local"
    }
}

/** When the sleep timer pauses playback. */
sealed interface Sleep {
    data object Off : Sleep
    data object EndOfSong : Sleep
    data class Minutes(val minutes: Int) : Sleep
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

    private val _autoplay = MutableStateFlow(store.getBoolean("autoplay", true))
    /** Whether the queue carries on with similar songs once it runs out. */
    val autoplay: StateFlow<Boolean> = _autoplay.asStateFlow()

    private val _normalise = MutableStateFlow(store.getBoolean("normalise", false))
    /** Whether every song plays at the same loudness. The engine reads it as it starts. */
    val normalise: StateFlow<Boolean> = _normalise.asStateFlow()

    fun setAutoplay(on: Boolean) {
        _autoplay.value = on
        store.edit().putBoolean("autoplay", on).apply()
    }

    fun setNormalise(on: Boolean) {
        _normalise.value = on
        store.edit().putBoolean("normalise", on).apply()
    }

    fun setFolders(folders: List<String>) {
        _folders.value = folders.distinct().sorted()
        store.edit().putStringSet("folders", folders.toSet()).apply()
    }
}

/** Starts a service the way Android 8 and later want one started while the app is in front. */
fun Context.startForegroundServiceCompat(intent: Intent) =
    ContextCompat.startForegroundService(this, intent)
