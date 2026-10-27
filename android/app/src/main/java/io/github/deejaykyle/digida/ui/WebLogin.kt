package io.github.deejaykyle.digida.ui

import android.annotation.SuppressLint
import android.webkit.CookieManager
import android.webkit.WebView
import android.webkit.WebViewClient
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.foundation.layout.width
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.rounded.Close
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.viewinterop.AndroidView
import io.github.deejaykyle.digida.R
import uniffi.digida_mobile.WebLogin

/**
 * A provider's own sign-in page in a web view, the way the desktop's sign-in window works. It
 * starts from no cookies, so an old session never signs in by accident, and answers with the
 * cookies of `login.domain` once they carry one of the names that prove the user got through.
 */
@SuppressLint("SetJavaScriptEnabled")
@Composable
fun WebLoginScreen(service: String, login: WebLogin, onCookies: (String) -> Unit, onCancel: () -> Unit) {
    val palette = LocalPalette.current
    Column(Modifier.fillMaxSize().background(palette.page).statusBarsPadding().navigationBarsPadding()) {
        Row(Modifier.padding(horizontal = 14.dp, vertical = 10.dp), verticalAlignment = Alignment.CenterVertically) {
            IconPill(Icons.Rounded.Close, stringResource(R.string.cancel), onCancel, size = 38.dp)
            Spacer(Modifier.width(12.dp))
            Label(service, weight = FontWeight.SemiBold)
        }
        AndroidView(
            modifier = Modifier.weight(1f),
            factory = { context ->
                val cookies = CookieManager.getInstance()
                cookies.removeAllCookies(null)
                cookies.setAcceptCookie(true)
                var answered = false
                WebView(context).apply {
                    cookies.setAcceptThirdPartyCookies(this, true)
                    settings.javaScriptEnabled = true
                    settings.domStorageEnabled = true
                    login.agent?.let { settings.userAgentString = it }
                    webViewClient = object : WebViewClient() {
                        override fun onPageFinished(view: WebView, url: String) {
                            if (answered) return
                            val header = cookies.getCookie("https://${login.domain.trimStart('.')}") ?: return
                            val names = header.split(";").map { it.trim().substringBefore("=") }
                            if (login.proof.any { it in names }) {
                                answered = true
                                cookies.flush()
                                onCookies(header)
                            }
                        }
                    }
                    loadUrl(login.url)
                }
            },
        )
    }
}
