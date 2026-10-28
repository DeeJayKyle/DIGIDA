package io.github.deejaykyle.digida

import android.annotation.SuppressLint
import android.content.Context
import android.net.Uri
import android.os.SystemClock
import android.util.Log
import android.webkit.CookieManager
import android.webkit.WebView
import android.webkit.WebViewClient
import androidx.webkit.WebViewCompat
import androidx.webkit.WebViewFeature
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import org.json.JSONObject
import uniffi.digida_mobile.Digida

/**
 * Mints YouTube's proof-of-origin tokens in a web view no one sees, the way the desktop's
 * hidden browser window does. The core records which identifier it is waiting on, this notices,
 * opens YouTube's embed player running the desktop's own minting script, reads the token the
 * script leaves in a cookie and files it with the core. A page that minted stays open a little
 * while for the next ask, then closes, since it holds a lot of memory.
 */
class PoTokenMinter(context: Context, private val core: Digida) {
    private val context = context.applicationContext
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.Main)
    private var open: Open? = null

    /** A minting page and where it is with its asks. */
    private class Open(
        val page: WebView,
        var warm: Boolean = false,
        var asked: Pair<String, Long>?,
        var used: Long,
    )

    /** Ticks for the life of the process: one tick starts a mint, carries one along, or idles. */
    fun start() {
        scope.launch {
            while (true) {
                delay(if (open != null) BUSY_TICK else TICK)
                runCatching { tick() }.onFailure { Log.w(TAG, "cannot mint: ${it.message}") }
            }
        }
    }

    private fun tick() {
        val current = open
        when {
            current == null -> begin()
            current.asked != null -> carry(current)
            else -> ask(current)
        }
    }

    /** Opens a page for whatever the core is waiting on, if anything. */
    @SuppressLint("SetJavaScriptEnabled")
    private fun begin() {
        val binding = core.potokenWanted() ?: return
        val page = runCatching { WebView(context) }.getOrElse {
            Log.w(TAG, "no web view to mint in: ${it.message}")
            core.potokenFailed()
            return
        }
        page.settings.javaScriptEnabled = true
        page.settings.domStorageEnabled = true
        page.settings.mediaPlaybackRequiresUserGesture = true
        // a page no one shows still runs its scripts, but some engines skip layout without a size
        page.layout(0, 0, PAGE_WIDTH, PAGE_HEIGHT)
        val script = core.potokenScript()
        if (WebViewFeature.isFeatureSupported(WebViewFeature.DOCUMENT_START_SCRIPT)) {
            WebViewCompat.addDocumentStartJavaScript(page, script, setOf(ORIGIN))
            page.webViewClient = WebViewClient()
        } else {
            page.webViewClient = object : WebViewClient() {
                override fun onPageStarted(view: WebView, url: String?, favicon: android.graphics.Bitmap?) {
                    view.evaluateJavascript(script, null)
                }
            }
        }
        page.loadUrl(core.potokenPage(binding))
        val now = SystemClock.elapsedRealtime()
        open = Open(page, asked = binding to now, used = now)
        Log.d(TAG, "minting for $binding")
    }

    /** Hands an attested page the next ask, or closes it once it has idled long enough. */
    private fun ask(current: Open) {
        if (SystemClock.elapsedRealtime() - current.used > LINGER) {
            close()
            return
        }
        val binding = core.potokenWanted() ?: return
        current.page.evaluateJavascript(
            "window.sonoraPoToken && window.sonoraPoToken(${JSONObject.quote(binding)})",
            null,
        )
        current.asked = binding to SystemClock.elapsedRealtime()
    }

    /** Looks for the page's answer to the ask in flight and files it. */
    private fun carry(current: Open) {
        val (binding, asked) = current.asked ?: return
        val waited = SystemClock.elapsedRealtime() - asked
        val answer = reply(CookieManager.getInstance().getCookie(ORIGIN), binding)
        when {
            answer == null && waited > (if (current.warm) ANSWER else PATIENCE) -> {
                Log.w(TAG, "the minting page did not answer in ${waited}ms")
                fail()
            }
            answer == null -> Unit
            answer.startsWith(FAILED) -> {
                Log.w(TAG, "the page could not mint a token: ${answer.drop(1)}")
                fail()
            }
            else -> {
                Log.d(TAG, "minted a token for $binding in ${waited}ms")
                current.warm = true
                current.asked = null
                current.used = SystemClock.elapsedRealtime()
                core.potokenGive(binding, answer)
                core.potokenSetWarm(true)
            }
        }
    }

    private fun fail() {
        close()
        core.potokenFailed()
    }

    /** Closes the page, which hands back everything the engine held for it. */
    private fun close() {
        open?.page?.destroy()
        open = null
        core.potokenSetWarm(false)
    }

    /** The answer for `binding` out of a `Cookie` header, or none while it holds an older one. */
    private fun reply(header: String?, binding: String): String? = header
        ?.split(';')
        ?.map { it.trim() }
        ?.firstOrNull { it.startsWith("$COOKIE=") }
        ?.substringAfter('=')
        ?.let { value ->
            val separator = value.indexOf(SEPARATOR)
            if (separator < 0 || Uri.decode(value.substring(0, separator)) != binding) null
            else value.substring(separator + 1)
        }

    companion object {
        private const val TAG = "digida-potoken"
        private const val ORIGIN = "https://www.youtube.com"
        /** The cookie the page leaves its answers in, and how an answer is laid out in it. */
        private const val COOKIE = "SONORA_POT"
        private const val FAILED = "!"
        private const val SEPARATOR = ':'
        private const val TICK = 500L
        private const val BUSY_TICK = 100L
        /** How long a fresh page gets to attest and mint, and an attested one to answer again. */
        private const val PATIENCE = 45_000L
        private const val ANSWER = 10_000L
        /** How long a page stays open after its last answer. */
        private const val LINGER = 20_000L
        private const val PAGE_WIDTH = 360
        private const val PAGE_HEIGHT = 640
    }
}
