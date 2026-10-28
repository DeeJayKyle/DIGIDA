package io.github.deejaykyle.digida.ui

import android.content.Intent
import android.net.Uri
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyListScope
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.rounded.KeyboardArrowRight
import androidx.compose.material.icons.rounded.AccountCircle
import androidx.compose.material.icons.rounded.Backup
import androidx.compose.material.icons.rounded.Extension
import androidx.compose.material.icons.rounded.Info
import androidx.compose.material.icons.rounded.Palette
import androidx.compose.material.icons.rounded.PlayArrow
import androidx.compose.material.icons.rounded.Storage
import androidx.compose.material3.Icon
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableLongStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import coil.imageLoader
import io.github.deejaykyle.digida.Library
import io.github.deejaykyle.digida.Preferences
import io.github.deejaykyle.digida.R
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import uniffi.digida_mobile.ScrobbleLink
import uniffi.digida_mobile.Scrobbler

/** The pages the settings split into, each under the heading it is listed beneath. */
enum class SettingsPage(val title: Int, val detail: Int, val icon: ImageVector, val group: Int) {
    Appearance(R.string.appearance, R.string.appearance_detail, Icons.Rounded.Palette, R.string.group_interface),
    Player(R.string.player_audio, R.string.player_audio_detail, Icons.Rounded.PlayArrow, R.string.group_player),
    Accounts(R.string.accounts, R.string.accounts_detail, Icons.Rounded.AccountCircle, R.string.group_services),
    Integrations(R.string.integrations, R.string.integrations_detail, Icons.Rounded.Extension, R.string.group_services),
    Storage(R.string.storage, R.string.storage_detail, Icons.Rounded.Storage, R.string.group_data),
    Backup(R.string.backup, R.string.backup_detail, Icons.Rounded.Backup, R.string.group_data),
    About(R.string.about_page, R.string.about_page_detail, Icons.Rounded.Info, R.string.group_system),
}

/** The settings' first page: every page as a row under its heading. */
fun LazyListScope.settingsIndex(onOpen: (SettingsPage) -> Unit) {
    SettingsPage.entries.groupBy { it.group }.forEach { (group, pages) ->
        item(key = "group$group") { Heading(stringResource(group), null) }
        pages.forEach { page ->
            item(key = page.name) { PageRow(page) { onOpen(page) } }
        }
    }
}

/** One page of the settings, as a row with its icon, name and what it holds. */
@Composable
private fun PageRow(page: SettingsPage, onClick: () -> Unit) {
    val palette = LocalPalette.current
    Pressable(
        onClick = onClick,
        modifier = Modifier.fillMaxWidth().padding(vertical = 5.dp),
        contentAlignment = Alignment.CenterStart,
    ) {
        Row(Modifier.padding(12.dp), verticalAlignment = Alignment.CenterVertically) {
            Box(
                Modifier.size(44.dp).surface(palette, Depth.Low, fill = palette.muted, radius = 12.dp),
                contentAlignment = Alignment.Center,
            ) {
                Icon(page.icon, null, tint = palette.text)
            }
            Spacer(Modifier.width(14.dp))
            Column(Modifier.weight(1f)) {
                Label(stringResource(page.title), weight = FontWeight.SemiBold)
                Label(stringResource(page.detail), size = 13.sp, subtle = true, lines = 2)
            }
            Icon(Icons.AutoMirrored.Rounded.KeyboardArrowRight, null, tint = palette.subtle)
        }
    }
}

/** The scrobbling services, each linked, switched and unlinked in its own row. */
@Composable
fun Scrobbling(library: Library) {
    val scrobblers by library.scrobblers.collectAsStateWithLifecycle()
    Column(verticalArrangement = Arrangement.spacedBy(10.dp)) {
        scrobblers.forEach { ScrobblerRow(it, library) }
    }
}

@Composable
private fun ScrobblerRow(scrobbler: Scrobbler, library: Library) {
    val palette = LocalPalette.current
    val context = LocalContext.current
    val scope = rememberCoroutineScope()
    var open by remember { mutableStateOf(false) }
    var busy by remember { mutableStateOf(false) }
    var problem by remember { mutableStateOf<String?>(null) }
    var key by remember { mutableStateOf("") }
    var secret by remember { mutableStateOf("") }
    var token by remember { mutableStateOf("") }
    var url by remember { mutableStateOf("") }

    Column(
        Modifier.fillMaxWidth().surface(palette, Depth.Low).padding(14.dp),
        verticalArrangement = Arrangement.spacedBy(10.dp),
    ) {
        Row(verticalAlignment = Alignment.CenterVertically) {
            Column(Modifier.weight(1f)) {
                Label(scrobblerName(scrobbler.id), weight = FontWeight.SemiBold)
                Label(
                    when {
                        busy -> stringResource(R.string.scrobble_waiting)
                        scrobbler.linked -> stringResource(R.string.scrobble_linked, scrobbler.name)
                        else -> stringResource(R.string.not_signed_in)
                    },
                    size = 13.sp,
                    subtle = true,
                )
            }
            when {
                scrobbler.linked -> SmallButton(stringResource(R.string.unlink)) { library.unlinkScrobbler(scrobbler.id) }
                !open -> SmallButton(stringResource(R.string.link), accent = true) { open = true }
            }
        }
        if (scrobbler.linked) {
            Toggle(stringResource(R.string.scrobble_on), stringResource(R.string.scrobble_on_detail), scrobbler.enabled) {
                library.setScrobblerEnabled(scrobbler.id, it)
            }
        }
        if (open && !scrobbler.linked) {
            when (scrobbler.link) {
                ScrobbleLink.KEYS -> {
                    Field(key, { key = it }, stringResource(R.string.api_key), Modifier.fillMaxWidth())
                    Field(secret, { secret = it }, stringResource(R.string.api_secret), Modifier.fillMaxWidth(), secret = true)
                }
                ScrobbleLink.TOKEN -> Field(token, { token = it }, stringResource(R.string.user_token), Modifier.fillMaxWidth(), secret = true)
                ScrobbleLink.SERVER -> {
                    Field(url, { url = it }, stringResource(R.string.server_address), Modifier.fillMaxWidth())
                    Field(key, { key = it }, stringResource(R.string.api_key), Modifier.fillMaxWidth(), secret = true)
                }
                ScrobbleLink.BROWSER -> Label(stringResource(R.string.scrobble_browser), size = 13.sp, subtle = true, lines = 3)
            }
            Row(horizontalArrangement = Arrangement.spacedBy(10.dp)) {
                scrobbler.signup?.let { page ->
                    SmallButton(stringResource(R.string.get_keys)) {
                        context.startActivity(Intent(Intent.ACTION_VIEW, Uri.parse(page)))
                    }
                }
                SmallButton(stringResource(R.string.link), accent = true) {
                    busy = true
                    problem = null
                    scope.launch {
                        problem = library.linkScrobbler(scrobbler.id, key, secret, token, url).exceptionOrNull()?.message
                        busy = false
                        if (problem == null) open = false
                    }
                }
            }
        }
        problem?.let { Label(it, size = 13.sp, subtle = true, lines = 4) }
    }
}

/** A scrobbling service's name. */
@Composable
private fun scrobblerName(id: String): String = when (id) {
    "lastfm" -> "Last.fm"
    "librefm" -> "Libre.fm"
    "listenbrainz" -> "ListenBrainz"
    "maloja" -> "Maloja"
    else -> id
}

/** How much the app keeps in its cache, and a button that clears the covers it fetched. */
@Composable
fun CacheSection() {
    val context = LocalContext.current
    val scope = rememberCoroutineScope()
    var size by remember { mutableLongStateOf(-1L) }
    LaunchedEffect(Unit) { size = withContext(Dispatchers.IO) { context.cacheDir.walkBottomUp().sumOf { it.length() } } }
    Column(verticalArrangement = Arrangement.spacedBy(10.dp)) {
        Label(
            if (size < 0) stringResource(R.string.loading) else stringResource(R.string.cache_size, size / (1024 * 1024)),
            subtle = true,
        )
        SmallButton(stringResource(R.string.clear_covers)) {
            scope.launch {
                withContext(Dispatchers.IO) {
                    context.imageLoader.diskCache?.clear()
                    context.imageLoader.memoryCache?.clear()
                }
                size = withContext(Dispatchers.IO) { context.cacheDir.walkBottomUp().sumOf { it.length() } }
            }
        }
    }
}

/** Saves the app's choices to a file, and takes them back from one. */
@Composable
fun BackupSection(preferences: Preferences) {
    val context = LocalContext.current
    var said by remember { mutableStateOf<String?>(null) }
    val saved = stringResource(R.string.backup_saved)
    val restored = stringResource(R.string.backup_restored)
    val save = rememberLauncherForActivityResult(ActivityResultContracts.CreateDocument("application/json")) { uri ->
        if (uri == null) return@rememberLauncherForActivityResult
        said = runCatching {
            context.contentResolver.openOutputStream(uri)?.use { it.write(preferences.export().toByteArray()) }
            saved
        }.getOrElse { it.message }
    }
    val load = rememberLauncherForActivityResult(ActivityResultContracts.OpenDocument()) { uri ->
        if (uri == null) return@rememberLauncherForActivityResult
        said = runCatching {
            val text = context.contentResolver.openInputStream(uri)?.use { it.readBytes().decodeToString() }.orEmpty()
            preferences.restore(text)
            restored
        }.getOrElse { it.message }
    }
    Column(verticalArrangement = Arrangement.spacedBy(10.dp)) {
        Row(horizontalArrangement = Arrangement.spacedBy(10.dp)) {
            SmallButton(stringResource(R.string.backup_save), accent = true) { save.launch("digida-settings.json") }
            SmallButton(stringResource(R.string.backup_restore)) { load.launch(arrayOf("application/json", "text/plain", "*/*")) }
        }
        said?.let { Label(it, size = 13.sp, subtle = true, lines = 3) }
    }
}

/** The app's name, version and licence. */
@Composable
fun AboutSection() {
    val context = LocalContext.current
    val version = remember {
        runCatching { context.packageManager.getPackageInfo(context.packageName, 0).versionName }.getOrNull().orEmpty()
    }
    Column(Modifier.padding(top = 12.dp), verticalArrangement = Arrangement.spacedBy(4.dp)) {
        Label(stringResource(R.string.app_name), size = 22.sp, weight = FontWeight.Bold)
        Label(stringResource(R.string.version, version), subtle = true)
        Label(stringResource(R.string.about), size = 13.sp, subtle = true, lines = 3)
    }
}

/** The changelog the app was built with, its headings set apart from its entries. */
@Composable
fun Changelog() {
    val palette = LocalPalette.current
    val lines = remember {
        // an entry that wraps carries on in indented lines, which join it again here
        uniffi.digida_mobile.changelog().lines()
            .dropWhile { !it.startsWith("## ") }
            .filter { it.isNotBlank() && !it.startsWith("[") }
            .fold(mutableListOf<String>()) { joined, line ->
                if (line.startsWith("  ") && joined.isNotEmpty()) joined[joined.lastIndex] += " " + line.trim()
                else joined += line
                joined
            }
    }
    Column(verticalArrangement = Arrangement.spacedBy(4.dp)) {
        lines.take(MAX_CHANGELOG_LINES).forEach { line ->
            when {
                line.startsWith("## ") -> Text(
                    line.removePrefix("## "),
                    Modifier.padding(top = 14.dp),
                    style = TextStyle(color = palette.text, fontSize = 17.sp, fontWeight = FontWeight.Bold),
                )
                line.startsWith("### ") -> Text(
                    line.removePrefix("### "),
                    Modifier.padding(top = 6.dp),
                    style = TextStyle(color = palette.text, fontSize = 14.sp, fontWeight = FontWeight.SemiBold),
                )
                else -> Text(
                    line.trim().removePrefix("- ").let { if (line.startsWith("- ")) "• $it" else it },
                    style = TextStyle(color = palette.subtle, fontSize = 13.sp),
                )
            }
        }
        Spacer(Modifier.height(8.dp))
    }
}

/** How much of the changelog the about page shows, which covers the last few releases. */
private const val MAX_CHANGELOG_LINES = 160
