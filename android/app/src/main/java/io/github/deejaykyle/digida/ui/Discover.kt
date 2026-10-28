package io.github.deejaykyle.digida.ui

import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.core.spring
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.slideInVertically
import androidx.compose.animation.slideOutVertically
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
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.LazyRow
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.lazy.itemsIndexed
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.rounded.ArrowBack
import androidx.compose.material.icons.automirrored.rounded.PlaylistAdd
import androidx.compose.material.icons.automirrored.rounded.QueueMusic
import androidx.compose.material.icons.rounded.Album
import androidx.compose.material.icons.rounded.Close
import androidx.compose.material.icons.rounded.Favorite
import androidx.compose.material.icons.rounded.Home
import androidx.compose.material.icons.rounded.LibraryMusic
import androidx.compose.material.icons.rounded.Person
import androidx.compose.material.icons.rounded.PlayArrow
import androidx.compose.material.icons.rounded.Search
import androidx.compose.material.icons.rounded.Settings
import androidx.compose.material.icons.rounded.Shuffle
import androidx.compose.material.icons.rounded.SkipNext
import androidx.compose.material3.Icon
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import io.github.deejaykyle.digida.Library
import io.github.deejaykyle.digida.R
import kotlinx.coroutines.launch
import uniffi.digida_mobile.ArtistPage
import uniffi.digida_mobile.Feed
import uniffi.digida_mobile.Release
import uniffi.digida_mobile.Song
import uniffi.digida_mobile.Status
import uniffi.digida_mobile.Tile
import uniffi.digida_mobile.TileKind
import uniffi.digida_mobile.Words

/** Where a song's menu and a feed card can take the app. */
interface Navigator {
    fun album(release: Release)
    fun playlist(id: String, title: String, cover: String?)
    fun artist(id: String, name: String)
}

/** The library song rows queue into when swiped, absent outside the app's own screens. */
val LocalLibrary = staticCompositionLocalOf<Library?> { null }

/** Opens a song's menu: play next, queue, its album and artist, and the heart. */
val LocalSongMenu = staticCompositionLocalOf<(Song) -> Unit> { {} }

/** The bottom tabs. */
enum class Tab(val label: Int, val icon: ImageVector) {
    Home(R.string.home, Icons.Rounded.Home),
    Search(R.string.search, Icons.Rounded.Search),
    Library(R.string.library, Icons.Rounded.LibraryMusic),
}

/** The tab bar along the bottom, the chosen tab pressed into it. */
@Composable
fun BottomBar(tab: Tab, onTab: (Tab) -> Unit) {
    val palette = LocalPalette.current
    Row(
        Modifier
            .fillMaxWidth()
            .navigationBarsPadding()
            .padding(horizontal = 14.dp, vertical = 8.dp)
            .surface(palette, Depth.Mid, fill = palette.raised, radius = 28.dp)
            .padding(6.dp),
        horizontalArrangement = Arrangement.SpaceEvenly,
    ) {
        Tab.entries.forEach { entry ->
            val chosen = entry == tab
            val lift by animateFloatAsState(if (chosen) 1f else 0f, spring(dampingRatio = 0.6f), label = "tab")
            Column(
                Modifier
                    .weight(1f)
                    .clip(RoundedCornerShape(22.dp))
                    .clickable { onTab(entry) }
                    .padding(vertical = 6.dp),
                horizontalAlignment = Alignment.CenterHorizontally,
            ) {
                Box(
                    Modifier
                        .graphicsLayer { scaleX = 1f + 0.08f * lift; scaleY = 1f + 0.08f * lift }
                        .then(
                            if (chosen) Modifier.surface(palette, Depth.Low, fill = palette.accent, radius = 16.dp)
                            else Modifier
                        )
                        .padding(horizontal = 18.dp, vertical = 4.dp),
                ) {
                    Icon(
                        entry.icon,
                        contentDescription = stringResource(entry.label),
                        tint = if (chosen) palette.onAccent else palette.subtle,
                        modifier = Modifier.size(22.dp),
                    )
                }
                Spacer(Modifier.height(3.dp))
                Label(stringResource(entry.label), size = 11.sp, subtle = !chosen, weight = if (chosen) FontWeight.SemiBold else FontWeight.Normal)
            }
        }
    }
}

/**
 * The Home tab: the shown service's feed of what was played lately, songs picked for the
 * listener and the rows it recommends. Local files have no feed, so their home is the newest
 * albums and a shuffle of the songs.
 */
@Composable
fun HomeFeedScreen(library: Library, navigator: Navigator, onSettings: () -> Unit) {
    val palette = LocalPalette.current
    val active by library.active.collectAsStateWithLifecycle()
    val shelf by library.shelf.collectAsStateWithLifecycle()
    var feed by remember(active) { mutableStateOf<Feed?>(null) }
    var failed by remember(active) { mutableStateOf<String?>(null) }
    LaunchedEffect(active) {
        if (active != Library.LOCAL) {
            runCatching { library.home() }.onSuccess { feed = it }.onFailure { failed = it.message }
        }
    }

    LazyColumn(contentPadding = PaddingValues(bottom = 24.dp)) {
        item {
            Row(Modifier.fillMaxWidth().padding(horizontal = 18.dp, vertical = 12.dp), verticalAlignment = Alignment.CenterVertically) {
                Label(stringResource(R.string.home), Modifier.weight(1f), size = 28.sp, weight = FontWeight.Bold)
                IconPill(Icons.Rounded.Settings, stringResource(R.string.settings), onSettings, size = 40.dp)
            }
        }
        item { ServiceChips(library, Modifier.padding(bottom = 8.dp)) }
        if (active == Library.LOCAL) {
            if (shelf.albums.isNotEmpty()) {
                item { RowTitle(stringResource(R.string.albums)) }
                item {
                    LazyRow(contentPadding = PaddingValues(horizontal = 18.dp), horizontalArrangement = Arrangement.spacedBy(14.dp)) {
                        items(shelf.albums.take(20), key = { it.id }) { album ->
                            Card(album.cover, album.title, album.artist) { navigator.album(album) }
                        }
                    }
                }
            }
            if (shelf.songs.isNotEmpty()) {
                item {
                    Pressable(
                        onClick = { library.play(shelf.songs.shuffled(), 0) },
                        modifier = Modifier.padding(18.dp).fillMaxWidth().height(52.dp),
                        fill = palette.accent,
                        radius = 26.dp,
                    ) {
                        Row(verticalAlignment = Alignment.CenterVertically) {
                            Icon(Icons.Rounded.Shuffle, null, tint = palette.onAccent)
                            Spacer(Modifier.width(8.dp))
                            Text(
                                stringResource(R.string.shuffle_everything),
                                style = TextStyle(color = palette.onAccent, fontWeight = FontWeight.SemiBold, fontSize = 16.sp),
                            )
                        }
                    }
                }
            }
            return@LazyColumn
        }
        val shown = feed
        when {
            shown == null && failed == null -> item { Label(stringResource(R.string.loading), Modifier.padding(18.dp), subtle = true) }
            shown == null -> item { Label(failed ?: "", Modifier.padding(18.dp), subtle = true, lines = 4) }
            else -> {
                if (shown.listenAgain.isNotEmpty()) {
                    item { RowTitle(stringResource(R.string.listen_again)) }
                    item { TileRow(shown.listenAgain, library, navigator) }
                }
                if (shown.quickPicks.isNotEmpty()) {
                    item { RowTitle(stringResource(R.string.quick_picks)) }
                    itemsIndexed(shown.quickPicks.take(8), key = { _, song -> "pick-" + song.id }) { index, song ->
                        val status by library.status.collectAsStateWithLifecycle()
                        SongRow(song, playing = status.song?.id == song.id) { library.play(shown.quickPicks, index) }
                    }
                }
                shown.sections.forEachIndexed { index, section ->
                    item(key = "title-$index") { RowTitle(section.title) }
                    item(key = "row-$index") { TileRow(section.tiles, library, navigator) }
                }
            }
        }
    }
}

@Composable
private fun RowTitle(title: String) {
    Label(title, Modifier.padding(start = 18.dp, end = 18.dp, top = 22.dp, bottom = 10.dp), size = 20.sp, weight = FontWeight.Bold)
}

/** A row of feed cards. A song plays with the row's other songs after it. */
@Composable
private fun TileRow(tiles: List<Tile>, library: Library, navigator: Navigator) {
    val songs = tiles.mapNotNull { it.song }
    LazyRow(contentPadding = PaddingValues(horizontal = 18.dp), horizontalArrangement = Arrangement.spacedBy(14.dp)) {
        items(tiles.filter { it.kind != TileKind.GENRE }, key = { it.kind.name + it.id }) { tile ->
            Card(tile.cover, tile.title, tile.subtitle, round = tile.kind == TileKind.ARTIST) {
                when (tile.kind) {
                    TileKind.SONG -> tile.song?.let { library.play(songs, songs.indexOf(it).coerceAtLeast(0)) }
                    TileKind.ALBUM -> navigator.album(Release(tile.id, tile.title, tile.subtitle, tile.cover, 0, 0u))
                    TileKind.PLAYLIST -> navigator.playlist(tile.id, tile.title, tile.cover)
                    TileKind.ARTIST -> navigator.artist(tile.id, tile.title)
                    TileKind.GENRE -> Unit
                }
            }
        }
    }
}

/** A cover with its name and a line under it, round for an artist. */
@Composable
private fun Card(cover: String?, title: String, subtitle: String, round: Boolean = false, onClick: () -> Unit) {
    val palette = LocalPalette.current
    Column(Modifier.width(148.dp).clickable(onClick = onClick)) {
        val radius = if (round) 74.dp else palette.radius
        Box(Modifier.fillMaxWidth().aspectRatio(1f).surface(palette, Depth.Mid, fill = palette.raised, radius = radius)) {
            Cover(cover, title, Modifier.fillMaxSize(), radius = radius)
        }
        Spacer(Modifier.height(8.dp))
        Label(title, weight = FontWeight.SemiBold, size = 14.sp)
        if (subtitle.isNotEmpty()) Label(subtitle, size = 12.sp, subtle = true)
    }
}

/** An artist's page: their picture, best-known songs and albums. */
@Composable
fun ArtistScreen(library: Library, id: String, name: String, navigator: Navigator, onBack: () -> Unit) {
    val palette = LocalPalette.current
    val status by library.status.collectAsStateWithLifecycle()
    var page by remember(id) { mutableStateOf<ArtistPage?>(null) }
    var failed by remember(id) { mutableStateOf<String?>(null) }
    LaunchedEffect(id) { runCatching { library.artist(id) }.onSuccess { page = it }.onFailure { failed = it.message } }

    LazyColumn(contentPadding = PaddingValues(bottom = 24.dp)) {
        item {
            Row(Modifier.padding(horizontal = 18.dp, vertical = 12.dp)) {
                IconPill(Icons.AutoMirrored.Rounded.ArrowBack, stringResource(R.string.back), onBack, size = 40.dp)
            }
        }
        item {
            Column(Modifier.fillMaxWidth(), horizontalAlignment = Alignment.CenterHorizontally) {
                Box(Modifier.size(200.dp).surface(palette, Depth.High, fill = palette.raised, radius = 100.dp)) {
                    Cover(page?.cover, name, Modifier.fillMaxSize(), radius = 100.dp)
                }
                Spacer(Modifier.height(16.dp))
                Label(page?.name ?: name, size = 28.sp, weight = FontWeight.Bold)
                failed?.let { Label(it, Modifier.padding(18.dp), subtle = true, lines = 3) }
                val top = page?.top.orEmpty()
                if (top.isNotEmpty()) {
                    Spacer(Modifier.height(14.dp))
                    Row(horizontalArrangement = Arrangement.spacedBy(14.dp)) {
                        IconPill(Icons.Rounded.PlayArrow, stringResource(R.string.play), { library.play(top, 0) }, size = 56.dp, accent = true)
                        IconPill(Icons.Rounded.Shuffle, stringResource(R.string.shuffle), { library.play(top.shuffled(), 0) }, size = 56.dp)
                    }
                }
            }
        }
        val shown = page ?: return@LazyColumn
        if (shown.top.isNotEmpty()) {
            item { RowTitle(stringResource(R.string.popular)) }
            itemsIndexed(shown.top.take(10), key = { _, song -> "top-" + song.id }) { index, song ->
                SongRow(song, playing = status.song?.id == song.id) { library.play(shown.top, index) }
            }
        }
        if (shown.albums.isNotEmpty()) {
            item { RowTitle(stringResource(R.string.albums)) }
            item {
                LazyRow(contentPadding = PaddingValues(horizontal = 18.dp), horizontalArrangement = Arrangement.spacedBy(14.dp)) {
                    items(shown.albums, key = { it.id }) { album ->
                        Card(album.cover, album.title, album.year.takeIf { it > 0 }?.toString() ?: "") { navigator.album(album) }
                    }
                }
            }
        }
        shown.biography?.takeIf { it.isNotBlank() }?.let { bio ->
            item { RowTitle(stringResource(R.string.about_artist)) }
            item { Label(bio, Modifier.padding(horizontal = 18.dp), subtle = true, lines = 12, size = 14.sp) }
        }
    }
}

/** A song row that queues the song when swiped right, the way the swipe springs back. */
@Composable
fun SwipeToQueue(song: Song, library: Library, content: @Composable () -> Unit) {
    val palette = LocalPalette.current
    var offset by remember { mutableFloatStateOf(0f) }
    val shown by animateFloatAsState(offset, spring(dampingRatio = 0.7f, stiffness = 500f), label = "swipe")
    val threshold = 140f
    Box {
        Row(
            Modifier.matchParentSize().padding(horizontal = 30.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Icon(
                Icons.AutoMirrored.Rounded.PlaylistAdd,
                null,
                tint = palette.accent.copy(alpha = (shown / threshold).coerceIn(0f, 1f)),
            )
        }
        Box(
            Modifier
                .graphicsLayer { translationX = shown }
                .pointerInput(song.id) {
                    detectHorizontalDragGestures(
                        onDragEnd = {
                            if (offset > threshold) library.addToQueue(song)
                            offset = 0f
                        },
                        onDragCancel = { offset = 0f },
                    ) { _, amount -> offset = (offset + amount).coerceIn(0f, threshold * 1.6f) }
                },
        ) { content() }
    }
}

/** What can be done with one song, raised from the bottom edge. */
@Composable
fun SongMenu(song: Song?, library: Library, navigator: Navigator, onClose: () -> Unit) {
    val palette = LocalPalette.current
    Box(Modifier.fillMaxSize(), contentAlignment = Alignment.BottomCenter) {
        AnimatedVisibility(song != null, enter = fadeIn(), exit = fadeOut()) {
            Box(Modifier.fillMaxSize().background(palette.page.copy(alpha = 0.55f)).clickable(onClick = onClose))
        }
        AnimatedVisibility(
            visible = song != null,
            enter = slideInVertically(spring(dampingRatio = 0.85f, stiffness = 420f)) { it },
            exit = slideOutVertically { it },
        ) {
            val shown = song ?: return@AnimatedVisibility
            Column(
                Modifier
                    .fillMaxWidth()
                    .navigationBarsPadding()
                    .padding(12.dp)
                    .surface(palette, Depth.High, fill = palette.raised)
                    .padding(18.dp),
                verticalArrangement = Arrangement.spacedBy(4.dp),
            ) {
                Row(verticalAlignment = Alignment.CenterVertically) {
                    Cover(shown.cover, shown.album, Modifier.size(52.dp), radius = 10.dp)
                    Spacer(Modifier.width(12.dp))
                    Column(Modifier.weight(1f)) {
                        Label(shown.title, weight = FontWeight.SemiBold)
                        Label(shown.artist, size = 13.sp, subtle = true)
                    }
                }
                Spacer(Modifier.height(8.dp))
                MenuItem(Icons.Rounded.SkipNext, stringResource(R.string.play_next)) { library.playNext(shown); onClose() }
                MenuItem(Icons.AutoMirrored.Rounded.PlaylistAdd, stringResource(R.string.add_to_queue)) { library.addToQueue(shown); onClose() }
                shown.albumId?.let { id ->
                    MenuItem(Icons.Rounded.Album, stringResource(R.string.go_to_album)) {
                        onClose()
                        navigator.album(Release(id, shown.album, shown.artist, shown.cover, 0, 0u))
                    }
                }
                shown.artistId?.let { id ->
                    MenuItem(Icons.Rounded.Person, stringResource(R.string.go_to_artist)) {
                        onClose()
                        navigator.artist(id, shown.artist.substringBefore(","))
                    }
                }
                if (shown.provider != Library.LOCAL) {
                    MenuItem(Icons.Rounded.Favorite, stringResource(R.string.like)) { library.setLiked(shown, true); onClose() }
                }
            }
        }
    }
}

@Composable
private fun MenuItem(icon: ImageVector, label: String, onClick: () -> Unit) {
    val palette = LocalPalette.current
    Row(
        Modifier.fillMaxWidth().clip(RoundedCornerShape(12.dp)).clickable(onClick = onClick).padding(horizontal = 8.dp, vertical = 12.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Icon(icon, null, tint = palette.text, modifier = Modifier.size(22.dp))
        Spacer(Modifier.width(16.dp))
        Label(label, size = 15.sp)
    }
}

/** The queue: what played, what plays now and what comes next. A song jumps there when tapped. */
@Composable
fun QueuePanel(status: Status, library: Library, onClose: () -> Unit) {
    val palette = LocalPalette.current
    var songs by remember { mutableStateOf<List<Song>>(emptyList()) }
    LaunchedEffect(status.queueLength, status.index) { songs = library.queue() }
    val listing = rememberLazyListState()
    LaunchedEffect(songs.size) { if (songs.isNotEmpty()) listing.scrollToItem(status.index.toInt().coerceAtMost(songs.lastIndex)) }

    Column(Modifier.fillMaxSize().padding(top = 8.dp)) {
        Row(Modifier.padding(horizontal = 18.dp, vertical = 8.dp), verticalAlignment = Alignment.CenterVertically) {
            Icon(Icons.AutoMirrored.Rounded.QueueMusic, null, tint = palette.text)
            Spacer(Modifier.width(10.dp))
            Label(stringResource(R.string.queue), Modifier.weight(1f), size = 22.sp, weight = FontWeight.Bold)
            IconPill(Icons.Rounded.Close, stringResource(R.string.back), onClose, size = 38.dp)
        }
        LazyColumn(state = listing, contentPadding = PaddingValues(bottom = 24.dp)) {
            itemsIndexed(songs) { index, song ->
                val current = index == status.index.toInt()
                Row(verticalAlignment = Alignment.CenterVertically) {
                    Box(Modifier.weight(1f)) {
                        SongRow(song, playing = current) { library.jump(index) }
                    }
                    if (!current) {
                        IconPill(Icons.Rounded.Close, stringResource(R.string.remove), {
                            library.removeFromQueue(index)
                            songs = songs.filterIndexed { at, _ -> at != index }
                        }, size = 34.dp, modifier = Modifier.padding(end = 14.dp))
                    }
                }
            }
        }
    }
}

/**
 * The song's lyrics. Synced lyrics follow the song, the line being sung lit and kept in view,
 * and a tap on a line plays from there.
 */
@Composable
fun LyricsPanel(status: Status, library: Library, onClose: () -> Unit) {
    val palette = LocalPalette.current
    val song = status.song
    var words by remember(song?.id) { mutableStateOf<Words?>(null) }
    var searched by remember(song?.id) { mutableStateOf(false) }
    LaunchedEffect(song?.id) {
        if (song != null) words = runCatching { library.lyrics(song) }.getOrNull()
        searched = true
    }
    val shown = words
    val at = status.positionMs.toLong()
    val current = shown?.takeIf { it.synced }?.lines?.indexOfLast { it.startMs.toLong() <= at } ?: -1
    val listing = rememberLazyListState()
    val scope = rememberCoroutineScope()
    LaunchedEffect(current) { if (current > 2) scope.launch { listing.animateScrollToItem(current - 2) } }

    Column(Modifier.fillMaxSize().padding(top = 8.dp)) {
        Row(Modifier.padding(horizontal = 18.dp, vertical = 8.dp), verticalAlignment = Alignment.CenterVertically) {
            Label(stringResource(R.string.lyrics), Modifier.weight(1f), size = 22.sp, weight = FontWeight.Bold)
            IconPill(Icons.Rounded.Close, stringResource(R.string.back), onClose, size = 38.dp)
        }
        when {
            shown == null && !searched -> Label(stringResource(R.string.loading), Modifier.padding(18.dp), subtle = true)
            shown == null -> Label(stringResource(R.string.no_lyrics), Modifier.padding(18.dp), subtle = true)
            else -> LazyColumn(state = listing, contentPadding = PaddingValues(horizontal = 22.dp, vertical = 12.dp)) {
                itemsIndexed(shown.lines) { index, line ->
                    val lit = index == current
                    val glow by animateFloatAsState(if (lit || !shown.synced) 1f else 0.42f, label = "line")
                    Text(
                        line.text.ifBlank { "♪" },
                        modifier = Modifier
                            .fillMaxWidth()
                            .graphicsLayer { alpha = glow }
                            .clickable(enabled = shown.synced) { library.seek(line.startMs.toLong()) }
                            .padding(vertical = 8.dp),
                        style = TextStyle(
                            color = if (lit) palette.text else palette.subtle,
                            fontSize = if (lit) 26.sp else 22.sp,
                            fontWeight = FontWeight.Bold,
                            textAlign = TextAlign.Start,
                        ),
                    )
                }
                item {
                    Label(stringResource(R.string.lyrics_source, shown.source), Modifier.padding(vertical = 18.dp), size = 12.sp, subtle = true)
                }
            }
        }
    }
}
