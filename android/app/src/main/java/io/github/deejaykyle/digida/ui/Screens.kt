package io.github.deejaykyle.digida.ui

import androidx.activity.compose.BackHandler
import androidx.compose.animation.AnimatedContent
import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.core.FastOutSlowInEasing
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.core.spring
import androidx.compose.animation.core.tween
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.scaleIn
import androidx.compose.animation.slideInHorizontally
import androidx.compose.animation.slideInVertically
import androidx.compose.animation.slideOutHorizontally
import androidx.compose.animation.slideOutVertically
import androidx.compose.animation.togetherWith
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.gestures.detectHorizontalDragGestures
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.aspectRatio
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.grid.GridCells
import androidx.compose.foundation.lazy.grid.LazyVerticalGrid
import androidx.compose.foundation.lazy.grid.itemsIndexed
import androidx.compose.foundation.lazy.itemsIndexed
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.BasicTextField
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.rounded.ArrowBack
import androidx.compose.material.icons.rounded.Close
import androidx.compose.material.icons.rounded.KeyboardArrowDown
import androidx.compose.material.icons.rounded.Pause
import androidx.compose.material.icons.automirrored.rounded.QueueMusic
import androidx.compose.material.icons.rounded.MoreHoriz
import androidx.compose.material.icons.rounded.Lyrics
import androidx.compose.material.icons.rounded.PlayArrow
import androidx.compose.material.icons.rounded.Refresh
import androidx.compose.material.icons.rounded.Search
import androidx.compose.material.icons.rounded.Settings
import androidx.compose.material.icons.rounded.Shuffle
import androidx.compose.material.icons.rounded.DownloadForOffline
import androidx.compose.material.icons.rounded.SaveAlt
import androidx.compose.material.icons.rounded.SkipNext
import androidx.compose.material.icons.rounded.SkipPrevious
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateListOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.layout.onSizeChanged
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import io.github.deejaykyle.digida.Library
import io.github.deejaykyle.digida.Preferences
import io.github.deejaykyle.digida.R
import uniffi.digida_mobile.PlayState
import uniffi.digida_mobile.Opened
import uniffi.digida_mobile.Release
import uniffi.digida_mobile.Song
import uniffi.digida_mobile.Status

/** Where the app is: a tab's own page, or a page opened from one. */
sealed interface Route {
    data object Feed : Route
    data object Home : Route
    data object Search : Route
    data class Album(val release: Release) : Route
    data class Playlist(val playlist: uniffi.digida_mobile.Playlist) : Route
    data class Artist(val id: String, val name: String) : Route
    data object Settings : Route
    data class SettingsSection(val page: SettingsPage) : Route
    data object Equalizer : Route
    data object Downloads : Route
}

/** The page each tab opens on. */
private fun Tab.root(): Route = when (this) {
    Tab.Home -> Route.Feed
    Tab.Search -> Route.Search
    Tab.Library -> Route.Home
}

/**
 * The whole app once it may read the music: the page, the tabs, the player, and the menus and
 * panels raised over them.
 */
@Composable
fun App(library: Library, preferences: Preferences) {
    val palette = LocalPalette.current
    val status by library.status.collectAsStateWithLifecycle()
    var tab by remember { mutableStateOf(Tab.Home) }
    val stack = remember { mutableStateListOf<Route>(Route.Feed) }
    var expanded by remember { mutableStateOf(false) }
    var menu by remember { mutableStateOf<Song?>(null) }
    val back: () -> Unit = { stack.removeAt(stack.lastIndex) }
    val navigator = remember {
        object : Navigator {
            override fun album(release: Release) {
                expanded = false
                stack.add(Route.Album(release))
            }

            override fun playlist(id: String, title: String, cover: String?) {
                expanded = false
                stack.add(Route.Playlist(uniffi.digida_mobile.Playlist(id, title, "", cover, 0u)))
            }

            override fun artist(id: String, name: String) {
                expanded = false
                stack.add(Route.Artist(id, name))
            }
        }
    }

    LaunchedEffect(Unit) { library.scan() }
    // a link shared to the app plays its song, or opens its album, artist or playlist
    val link by library.link.collectAsStateWithLifecycle()
    LaunchedEffect(link) {
        val wanted = library.takeLink() ?: return@LaunchedEffect
        library.openLink(wanted)
            .onSuccess { opened ->
                when (opened) {
                    is Opened.PlaySong -> {
                        library.play(listOf(opened.song), 0)
                        expanded = true
                    }
                    is Opened.ShowAlbum -> {
                        library.show(opened.provider)
                        navigator.album(opened.release)
                    }
                    is Opened.ShowArtist -> {
                        library.show(opened.provider)
                        navigator.artist(opened.id, opened.name)
                    }
                    is Opened.ShowPlaylist -> {
                        library.show(opened.provider)
                        expanded = false
                        stack.add(Route.Playlist(opened.playlist))
                    }
                }
            }
            .onFailure { library.report(it.message) }
    }
    BackHandler(enabled = menu != null || expanded || stack.size > 1 || tab != Tab.Home) {
        when {
            menu != null -> menu = null
            expanded -> expanded = false
            stack.size > 1 -> back()
            else -> {
                tab = Tab.Home
                stack.clear()
                stack.add(Route.Feed)
            }
        }
    }

    CompositionLocalProvider(LocalSongMenu provides { song -> menu = song }, LocalLibrary provides library) {
        Box(Modifier.fillMaxSize()) {
            Backdrop(palette)
            Column(Modifier.fillMaxSize().statusBarsPadding()) {
                AnimatedContent(
                    targetState = stack.last(),
                    modifier = Modifier.weight(1f),
                    transitionSpec = {
                        val forward = stack.size > 1
                        val sign = if (forward) 1 else -1
                        (slideInHorizontally(tween(380, easing = FastOutSlowInEasing)) { it / 4 * sign } +
                            fadeIn(tween(300)) + scaleIn(tween(380), initialScale = 0.98f))
                            .togetherWith(
                                slideOutHorizontally(tween(380, easing = FastOutSlowInEasing)) { -it / 6 * sign } +
                                    fadeOut(tween(200))
                            )
                    },
                    label = "route",
                ) { route ->
                    when (route) {
                        Route.Feed -> HomeFeedScreen(library, navigator, onSettings = { stack.add(Route.Settings) })
                        Route.Home -> HomeScreen(
                            library,
                            onAlbum = { stack.add(Route.Album(it)) },
                            onPlaylist = { stack.add(Route.Playlist(it)) },
                            onSearch = {
                                tab = Tab.Search
                                stack.clear()
                                stack.add(Route.Search)
                            },
                            onSettings = { stack.add(Route.Settings) },
                            onDownloads = { stack.add(Route.Downloads) },
                        )
                        is Route.Album -> AlbumScreen(library, route.release, onBack = back)
                        is Route.Playlist -> PlaylistScreen(library, route.playlist, onBack = back)
                        is Route.Artist -> ArtistScreen(library, route.id, route.name, navigator, onBack = back)
                        Route.Search -> SearchScreen(library, onBack = if (stack.size > 1) back else null) { songs, index ->
                            SongRow(songs[index], playing = status.song?.id == songs[index].id) { library.play(songs, index) }
                        }
                        Route.Settings -> SettingsScreen(
                            library,
                            preferences,
                            page = null,
                            onBack = back,
                            onOpen = { stack.add(Route.SettingsSection(it)) },
                            onEqualizer = { stack.add(Route.Equalizer) },
                            onDownloads = { stack.add(Route.Downloads) },
                        )
                        is Route.SettingsSection -> SettingsScreen(
                            library,
                            preferences,
                            page = route.page,
                            onBack = back,
                            onOpen = { stack.add(Route.SettingsSection(it)) },
                            onEqualizer = { stack.add(Route.Equalizer) },
                            onDownloads = { stack.add(Route.Downloads) },
                        )
                        Route.Equalizer -> EqualizerScreen(library, onBack = back)
                        Route.Downloads -> DownloadsScreen(library, onBack = back)
                    }
                }
                AnimatedVisibility(
                    visible = status.song != null,
                    enter = slideInVertically(spring(dampingRatio = 0.8f)) { it } + fadeIn(),
                    exit = slideOutVertically { it } + fadeOut(),
                ) {
                    MiniPlayer(status, library, onExpand = { expanded = true })
                }
                BottomBar(tab) { chosen ->
                    if (chosen == tab && stack.size == 1) return@BottomBar
                    tab = chosen
                    stack.clear()
                    stack.add(chosen.root())
                }
            }
            AnimatedVisibility(
                visible = expanded && status.song != null,
                enter = slideInVertically(spring(dampingRatio = 0.85f, stiffness = 380f)) { it } + fadeIn(),
                exit = slideOutVertically(tween(320)) { it } + fadeOut(tween(320)),
            ) {
                PlayerScreen(status, library, onClose = { expanded = false })
            }
            SongMenu(menu, library, navigator, onClose = { menu = null })
            SignInPrompts(library)
        }
    }
}

/** The top of a screen: a title, an optional way back and actions on the right. */
@Composable
internal fun TopBar(
    title: String,
    onBack: (() -> Unit)? = null,
    actions: @Composable () -> Unit = {},
) {
    Row(
        Modifier.fillMaxWidth().padding(horizontal = 18.dp, vertical = 12.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        if (onBack != null) {
            IconPill(Icons.AutoMirrored.Rounded.ArrowBack, stringResource(R.string.back), onBack, size = 40.dp)
            Spacer(Modifier.width(14.dp))
        }
        Label(title, Modifier.weight(1f), size = 28.sp, weight = FontWeight.Bold)
        actions()
    }
}

@Composable
private fun HomeScreen(
    library: Library,
    onAlbum: (Release) -> Unit,
    onPlaylist: (uniffi.digida_mobile.Playlist) -> Unit,
    onSearch: () -> Unit,
    onSettings: () -> Unit,
    onDownloads: () -> Unit,
) {
    val palette = LocalPalette.current
    val shelf by library.shelf.collectAsStateWithLifecycle()
    val active by library.active.collectAsStateWithLifecycle()
    var tab by remember { mutableStateOf(0) }

    Column(Modifier.fillMaxSize()) {
        TopBar(stringResource(R.string.library)) {
            if (active != Library.LOCAL) {
                IconPill(Icons.Rounded.Search, stringResource(R.string.search), onSearch, size = 40.dp)
                Spacer(Modifier.width(10.dp))
            }
            IconPill(Icons.Rounded.DownloadForOffline, stringResource(R.string.downloads), onDownloads, size = 40.dp)
            Spacer(Modifier.width(10.dp))
            IconPill(Icons.Rounded.Settings, stringResource(R.string.settings), onSettings, size = 40.dp)
        }
        ServiceChips(library, Modifier.padding(bottom = 12.dp))
        // a guest has no library, so its page is search, with nothing to go back to
        if (shelf.guest) {
            val status by library.status.collectAsStateWithLifecycle()
            SearchScreen(library, onBack = null) { songs, index ->
                SongRow(songs[index], playing = status.song?.id == songs[index].id) { library.play(songs, index) }
            }
            return@Column
        }
        Segmented(
            listOf(stringResource(R.string.albums), stringResource(R.string.songs), stringResource(R.string.playlists)),
            tab,
            onSelect = { tab = it },
            modifier = Modifier.padding(horizontal = 18.dp),
        )
        Spacer(Modifier.height(10.dp))
        when {
            shelf.loading && shelf.albums.isEmpty() && shelf.songs.isEmpty() -> Message(
                stringResource(if (active == Library.LOCAL) R.string.scanning else R.string.loading),
                null,
            )
            shelf.albums.isEmpty() && shelf.songs.isEmpty() && shelf.playlists.isEmpty() ->
                Message(
                    stringResource(R.string.empty_title),
                    shelf.error ?: stringResource(if (active == Library.LOCAL) R.string.empty_detail else R.string.empty_service),
                )
            tab == 0 -> AlbumGrid(shelf.albums, onAlbum)
            tab == 2 -> PlaylistGrid(shelf.playlists, onPlaylist)
            else -> SongList(shelf.songs, library, header = {
                Pressable(
                    onClick = { library.play(shelf.songs.shuffled(), 0) },
                    modifier = Modifier.padding(horizontal = 18.dp, vertical = 6.dp).height(48.dp).fillMaxWidth(),
                    fill = palette.accent,
                    radius = 24.dp,
                ) {
                    Row(verticalAlignment = Alignment.CenterVertically) {
                        androidx.compose.material3.Icon(Icons.Rounded.Shuffle, null, tint = palette.onAccent)
                        Spacer(Modifier.width(8.dp))
                        androidx.compose.material3.Text(
                            stringResource(R.string.shuffle),
                            style = TextStyle(color = palette.onAccent, fontWeight = FontWeight.SemiBold, fontSize = 16.sp),
                        )
                    }
                }
            })
        }
    }
}

/** A row of choices where the chosen one sits pressed into a shared track. */
@Composable
private fun Segmented(options: List<String>, selected: Int, onSelect: (Int) -> Unit, modifier: Modifier = Modifier) {
    val palette = LocalPalette.current
    Row(
        modifier
            .surface(palette, Depth.Low, pressed = palette.finish != Finish.Glass, fill = palette.muted, radius = 22.dp)
            .padding(4.dp),
    ) {
        options.forEachIndexed { index, option ->
            val chosen = index == selected
            Box(
                Modifier
                    .weight(1f)
                    .height(36.dp)
                    .then(
                        if (chosen) Modifier.surface(palette, Depth.Low, fill = palette.raised, radius = 18.dp)
                        else Modifier
                    )
                    .clip(RoundedCornerShape(18.dp))
                    .clickable { onSelect(index) },
                contentAlignment = Alignment.Center,
            ) {
                Label(option, size = 14.sp, weight = if (chosen) FontWeight.SemiBold else FontWeight.Normal, subtle = !chosen)
            }
        }
    }
}

@Composable
private fun Message(title: String, detail: String?) {
    Column(
        Modifier.fillMaxSize().padding(32.dp),
        verticalArrangement = Arrangement.Center,
        horizontalAlignment = Alignment.CenterHorizontally,
    ) {
        Label(title, size = 20.sp, weight = FontWeight.SemiBold)
        if (detail != null) {
            Spacer(Modifier.height(8.dp))
            Label(detail, subtle = true, lines = 4)
        }
    }
}

/** The albums as a grid of covers that rise into place as they first appear. */
@Composable
private fun AlbumGrid(albums: List<Release>, onAlbum: (Release) -> Unit) {
    val palette = LocalPalette.current
    LazyVerticalGrid(
        columns = GridCells.Adaptive(150.dp),
        contentPadding = PaddingValues(18.dp),
        horizontalArrangement = Arrangement.spacedBy(16.dp),
        verticalArrangement = Arrangement.spacedBy(20.dp),
    ) {
        itemsIndexed(albums, key = { _, album -> album.id }) { index, album ->
            Entrance(index) {
                Column(Modifier.clickable { onAlbum(album) }) {
                    Box(
                        Modifier
                            .fillMaxWidth()
                            .aspectRatio(1f)
                            .surface(palette, Depth.Mid, fill = palette.raised)
                    ) {
                        Cover(album.cover, album.title, Modifier.fillMaxSize(), radius = palette.radius)
                    }
                    Spacer(Modifier.height(10.dp))
                    Label(album.title, weight = FontWeight.SemiBold)
                    Label(album.artist, size = 13.sp, subtle = true)
                }
            }
        }
    }
}

/** Fades and lifts its content in the first time it is laid out, a little later down a list. */
@Composable
private fun Entrance(index: Int, content: @Composable () -> Unit) {
    var shown by remember { mutableStateOf(false) }
    LaunchedEffect(Unit) { shown = true }
    val delay = (index.coerceAtMost(12) * 35)
    val progress by animateFloatAsState(
        if (shown) 1f else 0f,
        tween(420, delayMillis = delay, easing = FastOutSlowInEasing),
        label = "entrance",
    )
    Box(
        Modifier.graphicsLayer {
            alpha = progress
            translationY = (1f - progress) * 24.dp.toPx()
            scaleX = 0.96f + 0.04f * progress
            scaleY = 0.96f + 0.04f * progress
        }
    ) { content() }
}

@Composable
private fun SongList(songs: List<Song>, library: Library, header: @Composable () -> Unit = {}) {
    val status by library.status.collectAsStateWithLifecycle()
    LazyColumn(contentPadding = PaddingValues(bottom = 18.dp)) {
        item { header() }
        itemsIndexed(songs, key = { _, song -> song.id }) { index, song ->
            Entrance(index) {
                SongRow(song, playing = status.song?.id == song.id) { library.play(songs, index) }
            }
        }
    }
}

@Composable
internal fun SongRow(song: Song, playing: Boolean, onClick: () -> Unit) {
    val library = LocalLibrary.current
    when (library) {
        null -> SongCard(song, playing, onClick)
        else -> SwipeToQueue(song, library) { SongCard(song, playing, onClick) }
    }
}

/** One song in a list: its cover, title, artist and length, its menu on a long press. */
@Composable
private fun SongCard(song: Song, playing: Boolean, onClick: () -> Unit) {
    val palette = LocalPalette.current
    val menu = LocalSongMenu.current
    Pressable(
        onClick = onClick,
        onLongClick = { menu(song) },
        modifier = Modifier.fillMaxWidth().padding(horizontal = 18.dp, vertical = 5.dp),
        depth = Depth.Low,
        selected = playing,
        fill = if (playing) palette.muted else palette.surface,
        contentAlignment = Alignment.CenterStart,
    ) {
        Row(Modifier.padding(10.dp), verticalAlignment = Alignment.CenterVertically) {
            Cover(song.cover, song.album, Modifier.size(46.dp), radius = 8.dp)
            Spacer(Modifier.width(12.dp))
            Column(Modifier.weight(1f)) {
                Label(song.title, weight = if (playing) FontWeight.Bold else FontWeight.Medium)
                Label(song.artist, size = 13.sp, subtle = true)
            }
            Label(clock(song.durationMs.toLong()), size = 13.sp, subtle = true)
        }
    }
}

@Composable
private fun AlbumScreen(library: Library, release: Release, onBack: () -> Unit) {
    val palette = LocalPalette.current
    var songs by remember { mutableStateOf<List<Song>>(emptyList()) }
    LaunchedEffect(release.id) { songs = runCatching { library.albumSongs(release) }.getOrDefault(emptyList()) }

    Column(Modifier.fillMaxSize()) {
        TopBar("", onBack = onBack)
        SongList(songs, library, header = {
            Column(
                Modifier.fillMaxWidth().padding(horizontal = 18.dp),
                horizontalAlignment = Alignment.CenterHorizontally,
            ) {
                Box(Modifier.size(220.dp).surface(palette, Depth.High, fill = palette.raised)) {
                    Cover(release.cover, release.title, Modifier.fillMaxSize(), radius = palette.radius)
                }
                Spacer(Modifier.height(18.dp))
                Label(release.title, size = 24.sp, weight = FontWeight.Bold, lines = 2)
                Label(
                    listOfNotNull(release.artist, release.year.takeIf { it > 0 }?.toString()).joinToString(" · "),
                    subtle = true,
                )
                Spacer(Modifier.height(16.dp))
                Row(horizontalArrangement = Arrangement.spacedBy(14.dp)) {
                    IconPill(Icons.Rounded.PlayArrow, stringResource(R.string.play), { library.play(songs, 0) }, size = 56.dp, accent = true)
                    IconPill(Icons.Rounded.Shuffle, stringResource(R.string.shuffle), { library.play(songs.shuffled(), 0) }, size = 56.dp)
                    DownloadAll(songs, library)
                }
                Spacer(Modifier.height(12.dp))
            }
        })
    }
}

@Composable
private fun PlaylistScreen(library: Library, playlist: uniffi.digida_mobile.Playlist, onBack: () -> Unit) {
    val palette = LocalPalette.current
    var songs by remember { mutableStateOf<List<Song>>(emptyList()) }
    LaunchedEffect(playlist.id) { songs = runCatching { library.playlistSongs(playlist) }.getOrDefault(emptyList()) }

    Column(Modifier.fillMaxSize()) {
        TopBar("", onBack = onBack)
        SongList(songs, library, header = {
            Column(
                Modifier.fillMaxWidth().padding(horizontal = 18.dp),
                horizontalAlignment = Alignment.CenterHorizontally,
            ) {
                Box(Modifier.size(220.dp).surface(palette, Depth.High, fill = palette.raised)) {
                    Cover(playlist.cover, playlist.title, Modifier.fillMaxSize(), radius = palette.radius)
                }
                Spacer(Modifier.height(18.dp))
                Label(playlist.title, size = 24.sp, weight = FontWeight.Bold, lines = 2)
                Label(playlist.owner, subtle = true)
                Spacer(Modifier.height(16.dp))
                Row(horizontalArrangement = Arrangement.spacedBy(14.dp)) {
                    IconPill(Icons.Rounded.PlayArrow, stringResource(R.string.play), { library.play(songs, 0) }, size = 56.dp, accent = true)
                    IconPill(Icons.Rounded.Shuffle, stringResource(R.string.shuffle), { library.play(songs.shuffled(), 0) }, size = 56.dp)
                    DownloadAll(songs, library)
                }
                Spacer(Modifier.height(12.dp))
            }
        })
    }
}

/** The bar over the bottom edge while something plays, its progress drawn along its foot. */
@Composable
private fun MiniPlayer(status: Status, library: Library, onExpand: () -> Unit) {
    val palette = LocalPalette.current
    val song = status.song ?: return
    val fraction = if (status.durationMs > 0u) status.positionMs.toFloat() / status.durationMs.toFloat() else 0f
    var dragged by remember { mutableStateOf(0f) }

    Pressable(
        onClick = onExpand,
        modifier = Modifier
            .fillMaxWidth()
            .padding(start = 12.dp, end = 12.dp, top = 8.dp)
            .pointerInput(Unit) {
                detectHorizontalDragGestures(
                    onDragEnd = {
                        if (dragged < -120f) library.next() else if (dragged > 120f) library.previous()
                        dragged = 0f
                    },
                ) { _, amount -> dragged += amount }
            },
        depth = Depth.High,
        fill = palette.raised,
    ) {
        Column {
            Row(Modifier.padding(10.dp), verticalAlignment = Alignment.CenterVertically) {
                Cover(song.cover, song.album, Modifier.size(44.dp), radius = 10.dp)
                Spacer(Modifier.width(12.dp))
                Column(Modifier.weight(1f)) {
                    Label(song.title, weight = FontWeight.SemiBold)
                    Label(song.artist, size = 13.sp, subtle = true)
                }
                IconPill(
                    if (status.state == PlayState.PLAYING) Icons.Rounded.Pause else Icons.Rounded.PlayArrow,
                    stringResource(if (status.state == PlayState.PLAYING) R.string.pause else R.string.play),
                    library::toggle,
                    size = 42.dp,
                    accent = true,
                )
                Spacer(Modifier.width(8.dp))
                IconPill(Icons.Rounded.SkipNext, stringResource(R.string.next), library::next, size = 42.dp)
            }
            Box(Modifier.fillMaxWidth().height(3.dp).background(palette.muted)) {
                Box(Modifier.fillMaxWidth(fraction.coerceIn(0f, 1f)).height(3.dp).background(palette.accent))
            }
        }
    }
}

/** The full-screen player: the cover large, the song, a seek bar and the transport. */
@Composable
private fun PlayerScreen(status: Status, library: Library, onClose: () -> Unit) {
    val palette = LocalPalette.current
    val song = status.song ?: return
    val playing = status.state == PlayState.PLAYING
    val breathe by animateFloatAsState(
        if (playing) 1f else 0.9f,
        spring(dampingRatio = 0.6f, stiffness = 120f),
        label = "breathe",
    )
    var scrub by remember { mutableStateOf<Float?>(null) }
    var panel by remember { mutableStateOf<Panel?>(null) }
    val songMenu = LocalSongMenu.current
    BackHandler(enabled = panel != null) { panel = null }
    val duration = status.durationMs.toFloat().coerceAtLeast(1f)
    val position = scrub ?: status.positionMs.toFloat()

    Box(Modifier.fillMaxSize()) {
        Backdrop(palette)
        Column(
            Modifier.fillMaxSize().statusBarsPadding().navigationBarsPadding().padding(horizontal = 26.dp),
            horizontalAlignment = Alignment.CenterHorizontally,
        ) {
            Row(Modifier.fillMaxWidth().padding(vertical = 12.dp), verticalAlignment = Alignment.CenterVertically) {
                IconPill(Icons.Rounded.KeyboardArrowDown, stringResource(R.string.back), onClose, size = 42.dp)
                Spacer(Modifier.weight(1f))
                Label(song.album, Modifier.weight(3f), size = 13.sp, subtle = true)
                Spacer(Modifier.weight(1f))
            }
            Spacer(Modifier.weight(1f))
            Box(
                Modifier
                    .fillMaxWidth()
                    .aspectRatio(1f)
                    .graphicsLayer { scaleX = breathe; scaleY = breathe }
                    .surface(palette, Depth.High, fill = palette.raised)
            ) {
                Cover(song.cover, song.album, Modifier.fillMaxSize(), radius = palette.radius)
            }
            Spacer(Modifier.weight(1f))
            AnimatedContent(
                targetState = song,
                transitionSpec = {
                    (slideInVertically { it / 3 } + fadeIn()).togetherWith(slideOutVertically { -it / 3 } + fadeOut())
                },
                label = "song",
            ) { shown ->
                Column(Modifier.fillMaxWidth()) {
                    Label(shown.title, size = 24.sp, weight = FontWeight.Bold)
                    Label(shown.artist, size = 16.sp, subtle = true)
                }
            }
            Spacer(Modifier.height(22.dp))
            Seeker(position / duration, onScrub = { scrub = it * duration }, onRelease = {
                scrub?.let { library.seek(it.toLong()) }
                scrub = null
            })
            Row(Modifier.fillMaxWidth().padding(top = 6.dp)) {
                Label(clock(position.toLong()), size = 12.sp, subtle = true)
                Spacer(Modifier.weight(1f))
                Label(clock(status.durationMs.toLong()), size = 12.sp, subtle = true)
            }
            Spacer(Modifier.height(18.dp))
            Row(
                Modifier.fillMaxWidth(),
                horizontalArrangement = Arrangement.SpaceEvenly,
                verticalAlignment = Alignment.CenterVertically,
            ) {
                IconPill(Icons.Rounded.SkipPrevious, stringResource(R.string.previous), library::previous, size = 60.dp)
                IconPill(
                    if (playing) Icons.Rounded.Pause else Icons.Rounded.PlayArrow,
                    stringResource(if (playing) R.string.pause else R.string.play),
                    library::toggle,
                    size = 84.dp,
                    accent = true,
                )
                IconPill(Icons.Rounded.SkipNext, stringResource(R.string.next), library::next, size = 60.dp)
            }
            Spacer(Modifier.height(18.dp))
            Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.SpaceEvenly) {
                IconPill(Icons.Rounded.Lyrics, stringResource(R.string.lyrics), { panel = Panel.Lyrics }, size = 46.dp)
                IconPill(Icons.Rounded.MoreHoriz, stringResource(R.string.more), { songMenu(song) }, size = 46.dp)
                IconPill(Icons.AutoMirrored.Rounded.QueueMusic, stringResource(R.string.queue), { panel = Panel.Queue }, size = 46.dp)
            }
            Spacer(Modifier.height(24.dp))
        }
        // the queue and the lyrics rise over the player, which stays playing underneath
        AnimatedVisibility(
            visible = panel != null,
            enter = slideInVertically(spring(dampingRatio = 0.85f, stiffness = 420f)) { it } + fadeIn(),
            exit = slideOutVertically(tween(280)) { it } + fadeOut(tween(280)),
        ) {
            // the panel paints its own opaque page, so nothing of the player reads through it
            Box(Modifier.fillMaxSize()) {
                Backdrop(palette)
                Box(Modifier.fillMaxSize().statusBarsPadding().navigationBarsPadding()) {
                    when (panel) {
                        Panel.Queue -> QueuePanel(status, library) { panel = null }
                        Panel.Lyrics -> LyricsPanel(status, library) { panel = null }
                        null -> Unit
                    }
                }
            }
        }
    }
}

/** What can rise over the player. */
private enum class Panel { Queue, Lyrics }

/** A seek track sunk into the page with a raised knob that follows the finger. */
@Composable
private fun Seeker(fraction: Float, onScrub: (Float) -> Unit, onRelease: () -> Unit) {
    val palette = LocalPalette.current
    var width by remember { mutableStateOf(1f) }
    Box(
        Modifier
            .fillMaxWidth()
            .height(28.dp)
            .onSizeChanged { width = it.width.toFloat() }
            .pointerInput(Unit) {
                detectHorizontalDragGestures(
                    onDragStart = { onScrub((it.x / width).coerceIn(0f, 1f)) },
                    onDragEnd = onRelease,
                    onDragCancel = onRelease,
                ) { change, _ -> onScrub((change.position.x / width).coerceIn(0f, 1f)) }
            },
        contentAlignment = Alignment.CenterStart,
    ) {
        Box(
            Modifier.fillMaxWidth().height(8.dp)
                .surface(palette, Depth.Low, pressed = true, fill = palette.muted, radius = 4.dp)
        )
        Box(
            Modifier.fillMaxWidth(fraction.coerceIn(0f, 1f)).height(8.dp)
                .clip(RoundedCornerShape(4.dp)).background(palette.accent)
        )
        Box(
            Modifier
                .graphicsLayer { translationX = fraction.coerceIn(0f, 1f) * width - 11.dp.toPx() }
                .size(22.dp)
                .surface(palette, Depth.Low, fill = palette.raised, radius = 11.dp)
        )
    }
}

@Composable
private fun SettingsScreen(
    library: Library,
    preferences: Preferences,
    page: SettingsPage?,
    onBack: () -> Unit,
    onOpen: (SettingsPage) -> Unit,
    onEqualizer: () -> Unit,
    onDownloads: () -> Unit,
) {
    val palette = LocalPalette.current
    val finish by preferences.finish.collectAsStateWithLifecycle()
    val mode by preferences.mode.collectAsStateWithLifecycle()
    val folders by preferences.foldersFlow.collectAsStateWithLifecycle()
    var draft by remember { mutableStateOf("") }

    Column(Modifier.fillMaxSize()) {
        TopBar(stringResource(page?.title ?: R.string.settings), onBack = onBack)
        LazyColumn(contentPadding = PaddingValues(horizontal = 18.dp, vertical = 8.dp)) {
            if (page == null) settingsIndex(onOpen)
            if (page == SettingsPage.Appearance) {
            item { Heading(stringResource(R.string.style), stringResource(R.string.style_detail)) }
            item {
                Column(verticalArrangement = Arrangement.spacedBy(14.dp)) {
                    Finish.entries.chunked(2).forEach { pair ->
                        Row(horizontalArrangement = Arrangement.spacedBy(14.dp)) {
                            pair.forEach { option ->
                                Box(Modifier.weight(1f)) {
                                    StyleCard(option, palette.dark, chosen = option == finish) { preferences.setFinish(option) }
                                }
                            }
                        }
                    }
                }
            }
            item { Heading(stringResource(R.string.theme), null) }
            item {
                Segmented(
                    Mode.entries.map { stringResource(it.label) },
                    Mode.entries.indexOf(mode),
                    onSelect = { preferences.setMode(Mode.entries[it]) },
                )
            }
            }
            if (page == SettingsPage.Player) {
            item { Heading(stringResource(R.string.playback), null) }
            item { PlaybackSettings(library, preferences) }
            item {
                val sound by library.sound.collectAsStateWithLifecycle()
                Pressable(
                    onClick = onEqualizer,
                    modifier = Modifier.fillMaxWidth().padding(top = 10.dp),
                    contentAlignment = Alignment.CenterStart,
                ) {
                    Row(Modifier.padding(14.dp), verticalAlignment = Alignment.CenterVertically) {
                        Column(Modifier.weight(1f)) {
                            Label(stringResource(R.string.equalizer), weight = FontWeight.SemiBold)
                            Label(
                                sound.headphone?.let { stringResource(R.string.headphone_applied, it) }
                                    ?: stringResource(R.string.equalizer_detail),
                                size = 13.sp,
                                subtle = true,
                                lines = 2,
                            )
                        }
                        Label(stringResource(if (sound.enabled) R.string.on else R.string.off), subtle = true)
                    }
                }
            }
            }
            if (page == SettingsPage.Accounts) {
            item { Heading(stringResource(R.string.accounts), stringResource(R.string.accounts_detail)) }
            item { Accounts(library) }
            }
            if (page == SettingsPage.Integrations) {
            item { Heading(stringResource(R.string.scrobbling), stringResource(R.string.scrobbling_detail)) }
            item { Scrobbling(library) }
            }
            if (page == SettingsPage.Storage) {
            item {
                Pressable(
                    onClick = onDownloads,
                    modifier = Modifier.fillMaxWidth().padding(top = 10.dp),
                    contentAlignment = Alignment.CenterStart,
                ) {
                    Column(Modifier.padding(14.dp)) {
                        Label(stringResource(R.string.downloads), weight = FontWeight.SemiBold)
                        Label(stringResource(R.string.downloads_detail), size = 13.sp, subtle = true, lines = 2)
                    }
                }
            }
            item { Heading(stringResource(R.string.folders), stringResource(R.string.folders_detail)) }
            itemsIndexed(folders) { _, folder ->
                Row(
                    Modifier.fillMaxWidth().padding(vertical = 5.dp)
                        .surface(palette, Depth.Low).padding(start = 14.dp, end = 6.dp, top = 6.dp, bottom = 6.dp),
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    Label(folder, Modifier.weight(1f), size = 14.sp)
                    IconPill(Icons.Rounded.Close, stringResource(R.string.remove), {
                        preferences.setFolders(folders - folder)
                        library.scan()
                    }, size = 34.dp)
                }
            }
            item {
                Row(Modifier.padding(vertical = 6.dp), verticalAlignment = Alignment.CenterVertically) {
                    BasicTextField(
                        draft,
                        onValueChange = { draft = it },
                        singleLine = true,
                        textStyle = TextStyle(color = palette.text, fontSize = 14.sp),
                        cursorBrush = SolidColor(palette.accent),
                        modifier = Modifier.weight(1f)
                            .surface(palette, Depth.Low, pressed = palette.finish != Finish.Glass, fill = palette.muted)
                            .padding(14.dp),
                        decorationBox = { field ->
                            if (draft.isEmpty()) Label("/storage/emulated/0/Music", subtle = true, size = 14.sp)
                            field()
                        },
                    )
                    Spacer(Modifier.width(10.dp))
                    Pressable(
                        onClick = {
                            if (draft.isNotBlank()) {
                                preferences.setFolders(folders + draft.trim())
                                draft = ""
                                library.scan()
                            }
                        },
                        modifier = Modifier.height(46.dp).width(76.dp),
                        fill = palette.accent,
                    ) {
                        androidx.compose.material3.Text(
                            stringResource(R.string.add),
                            style = TextStyle(color = palette.onAccent, fontWeight = FontWeight.SemiBold),
                        )
                    }
                }
            }
            item {
                Pressable(
                    onClick = library::scan,
                    modifier = Modifier.padding(vertical = 10.dp).fillMaxWidth().height(48.dp),
                ) {
                    Row(verticalAlignment = Alignment.CenterVertically) {
                        androidx.compose.material3.Icon(Icons.Rounded.Refresh, null, tint = palette.text)
                        Spacer(Modifier.width(8.dp))
                        Label(stringResource(R.string.rescan), weight = FontWeight.Medium)
                    }
                }
            }
            item { Heading(stringResource(R.string.cache), stringResource(R.string.cache_detail)) }
            item { CacheSection() }
            }
            if (page == SettingsPage.Backup) {
            item { Heading(stringResource(R.string.backup), stringResource(R.string.backup_detail)) }
            item { BackupSection(preferences) }
            }
            if (page == SettingsPage.About) {
            item { AboutSection() }
            item { Heading(stringResource(R.string.supported_links), stringResource(R.string.supported_links_detail)) }
            item { Heading(stringResource(R.string.diagnostics), stringResource(R.string.diagnostics_detail)) }
            item { Diagnostics() }
            item { Heading(stringResource(R.string.changelog), null) }
            item { Changelog() }
            }
            item { Spacer(Modifier.height(120.dp)) }
        }
    }
}

/**
 * Downloads every song of an album or playlist: kept offline from a streaming service, or saved
 * as files from the listener's own server.
 */
@Composable
private fun DownloadAll(songs: List<Song>, library: Library) {
    val first = songs.firstOrNull() ?: return
    if (first.provider == Library.LOCAL) return
    when (library.savesFiles(first)) {
        true -> IconPill(Icons.Rounded.SaveAlt, stringResource(R.string.save_all), { library.saveFiles(songs) }, size = 56.dp)
        false -> IconPill(Icons.Rounded.DownloadForOffline, stringResource(R.string.download_all), { library.download(songs) }, size = 56.dp)
    }
}

@Composable
internal fun Heading(title: String, detail: String?) {
    Column(Modifier.padding(top = 22.dp, bottom = 12.dp)) {
        Label(title.uppercase(), size = 12.sp, weight = FontWeight.Bold, subtle = true)
        if (detail != null) Label(detail, size = 13.sp, subtle = true, lines = 2)
    }
}

/** One finish in miniature, drawn in its own palette whatever the app wears now. */
@Composable
private fun StyleCard(finish: Finish, dark: Boolean, chosen: Boolean, onClick: () -> Unit) {
    val current = LocalPalette.current
    val own = palette(finish, dark)
    val ring by animateFloatAsState(if (chosen) 1f else 0f, spring(dampingRatio = 0.6f), label = "ring")
    Column(horizontalAlignment = Alignment.CenterHorizontally) {
        Box(
            Modifier
                .fillMaxWidth()
                .aspectRatio(0.95f)
                .graphicsLayer { scaleX = 0.97f + 0.03f * ring; scaleY = 0.97f + 0.03f * ring }
                .surface(current, Depth.Mid, pressed = chosen && current.finish != Finish.Glass, fill = current.surface)
                .clickable(onClick = onClick)
                .padding(6.dp)
        ) {
            androidx.compose.runtime.CompositionLocalProvider(LocalPalette provides own) {
                Box(Modifier.fillMaxSize().clip(RoundedCornerShape(own.radius))) {
                    Backdrop(own)
                    Column(Modifier.fillMaxSize().padding(10.dp), verticalArrangement = Arrangement.SpaceBetween) {
                        Row(
                            Modifier.fillMaxWidth().surface(own, Depth.Mid, fill = own.raised, radius = own.radius * 0.6f)
                                .padding(8.dp),
                            verticalAlignment = Alignment.CenterVertically,
                        ) {
                            Box(Modifier.size(26.dp).clip(RoundedCornerShape(6.dp)).background(own.accent))
                            Spacer(Modifier.width(8.dp))
                            Column(Modifier.weight(1f)) {
                                Box(Modifier.fillMaxWidth(0.8f).height(5.dp).clip(RoundedCornerShape(3.dp)).background(own.text.copy(alpha = 0.8f)))
                                Spacer(Modifier.height(4.dp))
                                Box(Modifier.fillMaxWidth(0.5f).height(5.dp).clip(RoundedCornerShape(3.dp)).background(own.subtle.copy(alpha = 0.7f)))
                            }
                        }
                        Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.SpaceBetween) {
                            Box(Modifier.width(56.dp).height(16.dp).surface(own, Depth.Low, fill = own.accent, radius = 8.dp))
                            Box(Modifier.width(34.dp).height(16.dp).surface(own, Depth.Low, pressed = true, fill = own.muted, radius = 8.dp))
                        }
                    }
                }
            }
            if (chosen) {
                Box(
                    Modifier.align(Alignment.TopEnd).padding(4.dp).size(10.dp)
                        .clip(RoundedCornerShape(5.dp)).background(current.accent)
                )
            }
        }
        Spacer(Modifier.height(8.dp))
        Label(
            stringResource(finish.label),
            size = 14.sp,
            weight = if (chosen) FontWeight.Bold else FontWeight.Normal,
            subtle = !chosen,
        )
    }
}
