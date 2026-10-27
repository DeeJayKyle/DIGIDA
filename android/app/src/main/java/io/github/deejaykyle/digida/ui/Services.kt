package io.github.deejaykyle.digida.ui

import android.net.Uri
import androidx.browser.customtabs.CustomTabsIntent
import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.slideInVertically
import androidx.compose.animation.slideOutVertically
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.horizontalScroll
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
import androidx.compose.foundation.lazy.grid.GridCells
import androidx.compose.foundation.lazy.grid.LazyVerticalGrid
import androidx.compose.foundation.lazy.grid.itemsIndexed
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.BasicTextField
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.rounded.ArrowBack
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.input.PasswordVisualTransformation
import androidx.compose.ui.text.input.VisualTransformation
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import io.github.deejaykyle.digida.Library
import io.github.deejaykyle.digida.R
import kotlinx.coroutines.launch
import uniffi.digida_mobile.Method
import uniffi.digida_mobile.Playlist
import uniffi.digida_mobile.Prompt
import uniffi.digida_mobile.Service
import uniffi.digida_mobile.Song

/** The services the library can show, local files first, as a row of chips. */
@Composable
fun ServiceChips(library: Library, modifier: Modifier = Modifier) {
    val palette = LocalPalette.current
    val services by library.services.collectAsStateWithLifecycle()
    val active by library.active.collectAsStateWithLifecycle()
    val shown = services.filter { it.slug == Library.LOCAL || it.signedIn }
    if (shown.size < 2) return

    Row(
        modifier.horizontalScroll(rememberScrollState()).padding(horizontal = 18.dp),
        horizontalArrangement = Arrangement.spacedBy(10.dp),
    ) {
        shown.forEach { service ->
            val chosen = service.slug == active
            Pressable(
                onClick = { library.show(service.slug) },
                modifier = Modifier.height(36.dp),
                selected = chosen,
                fill = if (chosen) palette.accent else palette.surface,
                radius = 18.dp,
            ) {
                Text(
                    service.name,
                    modifier = Modifier.padding(horizontal = 16.dp),
                    style = TextStyle(
                        color = if (chosen) palette.onAccent else palette.text,
                        fontWeight = if (chosen) FontWeight.SemiBold else FontWeight.Medium,
                        fontSize = 14.sp,
                    ),
                )
            }
        }
    }
}

/** The playlists of the shown service, as a grid of covers. */
@Composable
fun PlaylistGrid(playlists: List<Playlist>, onPlaylist: (Playlist) -> Unit) {
    val palette = LocalPalette.current
    LazyVerticalGrid(
        columns = GridCells.Adaptive(150.dp),
        contentPadding = PaddingValues(18.dp),
        horizontalArrangement = Arrangement.spacedBy(16.dp),
        verticalArrangement = Arrangement.spacedBy(20.dp),
    ) {
        itemsIndexed(playlists, key = { _, playlist -> playlist.id }) { _, playlist ->
            Column(Modifier.clickable { onPlaylist(playlist) }) {
                Box(Modifier.fillMaxWidth().aspectRatio(1f).surface(palette, Depth.Mid, fill = palette.raised)) {
                    Cover(playlist.cover, playlist.title, Modifier.fillMaxSize(), radius = palette.radius)
                }
                Spacer(Modifier.height(10.dp))
                Label(playlist.title, weight = FontWeight.SemiBold)
                Label(playlist.owner, size = 13.sp, subtle = true)
            }
        }
    }
}

/** A text field sunk into the page, in the active finish. */
@Composable
fun Field(
    value: String,
    onChange: (String) -> Unit,
    hint: String,
    modifier: Modifier = Modifier,
    secret: Boolean = false,
    onDone: (() -> Unit)? = null,
) {
    val palette = LocalPalette.current
    BasicTextField(
        value,
        onValueChange = onChange,
        singleLine = true,
        textStyle = TextStyle(color = palette.text, fontSize = 15.sp),
        cursorBrush = SolidColor(palette.accent),
        visualTransformation = if (secret) PasswordVisualTransformation() else VisualTransformation.None,
        keyboardOptions = KeyboardOptions(
            keyboardType = if (secret) KeyboardType.Password else KeyboardType.Text,
            imeAction = if (onDone != null) ImeAction.Search else ImeAction.Next,
        ),
        keyboardActions = KeyboardActions(onSearch = { onDone?.invoke() }),
        modifier = modifier
            .surface(palette, Depth.Low, pressed = palette.finish != Finish.Glass, fill = palette.muted)
            .padding(14.dp),
        decorationBox = { field ->
            if (value.isEmpty()) Label(hint, subtle = true, size = 15.sp)
            field()
        },
    )
}

/** Search across the shown service, playing a result with the rest of the results after it. */
@Composable
fun SearchScreen(library: Library, onBack: () -> Unit, songRow: @Composable (List<Song>, Int) -> Unit) {
    val scope = rememberCoroutineScope()
    var query by remember { mutableStateOf("") }
    var results by remember { mutableStateOf<List<Song>>(emptyList()) }
    var busy by remember { mutableStateOf(false) }
    val run = {
        if (query.isNotBlank()) {
            busy = true
            scope.launch {
                results = runCatching { library.search(query.trim()) }.getOrDefault(emptyList())
                busy = false
            }
        }
    }

    Column(Modifier.fillMaxSize()) {
        Row(Modifier.padding(horizontal = 18.dp, vertical = 12.dp), verticalAlignment = Alignment.CenterVertically) {
            IconPill(Icons.AutoMirrored.Rounded.ArrowBack, stringResource(R.string.back), onBack, size = 40.dp)
            Spacer(Modifier.width(12.dp))
            Field(query, { query = it }, stringResource(R.string.search_hint), Modifier.weight(1f), onDone = { run() })
        }
        if (busy) Label(stringResource(R.string.searching), Modifier.padding(18.dp), subtle = true)
        LazyColumn {
            items(results.size) { index -> songRow(results, index) }
        }
    }
}

/** Every service, with a way in or out of each, for the settings. */
@Composable
fun Accounts(library: Library) {
    val services by library.services.collectAsStateWithLifecycle()
    val signing by library.signing.collectAsStateWithLifecycle()
    Column(verticalArrangement = Arrangement.spacedBy(10.dp)) {
        services.filter { it.slug != Library.LOCAL }.forEach { service ->
            Account(service, library, busy = signing == service.slug)
        }
    }
}

@Composable
private fun Account(service: Service, library: Library, busy: Boolean) {
    val palette = LocalPalette.current
    var form by remember { mutableStateOf(false) }
    Column(Modifier.fillMaxWidth().surface(palette, Depth.Low).padding(14.dp)) {
        Row(verticalAlignment = Alignment.CenterVertically) {
            Column(Modifier.weight(1f)) {
                Label(service.name, weight = FontWeight.SemiBold)
                Label(
                    when {
                        busy -> stringResource(R.string.signing_in)
                        service.signedIn -> service.account ?: stringResource(R.string.signed_in)
                        else -> stringResource(R.string.not_signed_in)
                    },
                    size = 13.sp,
                    subtle = true,
                )
            }
            when {
                busy -> SmallButton(stringResource(R.string.cancel)) { library.cancelSignIn() }
                service.signedIn -> SmallButton(stringResource(R.string.sign_out)) { library.signOut(service.slug) }
                else -> Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                    service.methods.forEach { method ->
                        when (method) {
                            is Method.Browser -> SmallButton(stringResource(R.string.sign_in), accent = true) {
                                library.signIn(service.slug, method)
                            }
                            is Method.Guest -> SmallButton(stringResource(R.string.guest)) {
                                library.signIn(service.slug, method)
                            }
                            is Method.Credentials -> SmallButton(stringResource(R.string.sign_in), accent = true) {
                                form = !form
                            }
                            is Method.Cookies -> SmallButton(stringResource(R.string.sign_in), accent = true) {
                                library.signIn(service.slug, method)
                            }
                        }
                    }
                }
            }
        }
        AnimatedVisibility(form && !service.signedIn) {
            CredentialsForm { server, user, password ->
                form = false
                library.signIn(service.slug, Method.Credentials(server, user, password))
            }
        }
    }
}

/** A server address, user name and password, as a self-hosted server signs in with. */
@Composable
private fun CredentialsForm(onSubmit: (String, String, String) -> Unit) {
    var server by remember { mutableStateOf("") }
    var user by remember { mutableStateOf("") }
    var password by remember { mutableStateOf("") }
    Column(Modifier.padding(top = 12.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
        Field(server, { server = it }, "https://music.example.com", Modifier.fillMaxWidth())
        Field(user, { user = it }, stringResource(R.string.username), Modifier.fillMaxWidth())
        Field(password, { password = it }, stringResource(R.string.password), Modifier.fillMaxWidth(), secret = true)
        SmallButton(stringResource(R.string.sign_in), accent = true) {
            if (server.isNotBlank() && user.isNotBlank()) onSubmit(server.trim(), user.trim(), password)
        }
    }
}

@Composable
fun SmallButton(text: String, accent: Boolean = false, onClick: () -> Unit) {
    val palette = LocalPalette.current
    Pressable(
        onClick = onClick,
        modifier = Modifier.height(36.dp),
        fill = if (accent) palette.accent else palette.surface,
        radius = 18.dp,
    ) {
        Text(
            text,
            modifier = Modifier.padding(horizontal = 14.dp),
            style = TextStyle(
                color = if (accent) palette.onAccent else palette.text,
                fontWeight = FontWeight.SemiBold,
                fontSize = 13.sp,
            ),
        )
    }
}

/**
 * Answers what a running sign-in asks: opens its page in a browser tab, shows a code to enter,
 * or lets the listener pick an account. Problems a sign-in ran into show along the bottom.
 */
@Composable
fun SignInPrompts(library: Library) {
    val palette = LocalPalette.current
    val context = LocalContext.current
    val asking by library.asking.collectAsStateWithLifecycle()
    val problem by library.problem.collectAsStateWithLifecycle()

    LaunchedEffect(asking) {
        val prompt = asking?.prompt
        if (prompt is Prompt.Url) {
            CustomTabsIntent.Builder().setShowTitle(true).build().launchUrl(context, Uri.parse(prompt.url))
        }
    }

    val cookies = asking?.takeIf { it.prompt is Prompt.Cookies }
    val login = remember(cookies?.service) { cookies?.let { library.webLogin(it.service) } }
    if (cookies != null && login != null) {
        val name = library.services.value.firstOrNull { it.slug == cookies.service }?.name ?: cookies.service
        WebLoginScreen(name, login, onCookies = library::answer, onCancel = library::cancelSignIn)
        return
    }

    Box(Modifier.fillMaxSize(), contentAlignment = Alignment.BottomCenter) {
        val prompt = asking?.prompt
        AnimatedVisibility(
            visible = prompt is Prompt.Code || prompt is Prompt.Accounts,
            enter = slideInVertically { it } + fadeIn(),
            exit = slideOutVertically { it } + fadeOut(),
        ) {
            Column(
                Modifier
                    .fillMaxWidth()
                    .navigationBarsPadding()
                    .padding(14.dp)
                    .surface(palette, Depth.High, fill = palette.raised)
                    .padding(18.dp),
                verticalArrangement = Arrangement.spacedBy(10.dp),
            ) {
                when (prompt) {
                    is Prompt.Code -> {
                        Label(stringResource(R.string.enter_code), weight = FontWeight.SemiBold)
                        Label(prompt.code, size = 30.sp, weight = FontWeight.Black)
                        SmallButton(stringResource(R.string.open_page), accent = true) {
                            CustomTabsIntent.Builder().build().launchUrl(context, Uri.parse(prompt.url))
                        }
                    }
                    is Prompt.Accounts -> {
                        Label(stringResource(R.string.pick_account), weight = FontWeight.SemiBold)
                        prompt.accounts.forEach { account ->
                            Column(
                                Modifier.fillMaxWidth().clip(RoundedCornerShape(12.dp))
                                    .clickable { library.answer(account.id) }
                                    .padding(10.dp)
                            ) {
                                Label(account.name, weight = FontWeight.Medium)
                                account.detail?.let { Label(it, size = 13.sp, subtle = true) }
                            }
                        }
                    }
                    else -> Unit
                }
                SmallButton(stringResource(R.string.cancel)) { library.cancelSignIn() }
            }
        }
        AnimatedVisibility(
            visible = problem != null,
            enter = slideInVertically { it } + fadeIn(),
            exit = slideOutVertically { it } + fadeOut(),
        ) {
            Row(
                Modifier
                    .fillMaxWidth()
                    .navigationBarsPadding()
                    .padding(14.dp)
                    .surface(palette, Depth.High, fill = palette.raised)
                    .clickable { library.dismissProblem() }
                    .padding(16.dp),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                Box(Modifier.size(10.dp).clip(RoundedCornerShape(5.dp)).background(Color(0xFFE5484D)))
                Spacer(Modifier.width(12.dp))
                Label(problem ?: "", Modifier.weight(1f), lines = 3, size = 14.sp)
            }
        }
    }
}
