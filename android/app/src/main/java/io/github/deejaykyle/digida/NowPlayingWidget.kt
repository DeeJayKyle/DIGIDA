package io.github.deejaykyle.digida

import android.app.PendingIntent
import android.appwidget.AppWidgetManager
import android.appwidget.AppWidgetProvider
import android.content.ComponentName
import android.content.Context
import android.content.Intent
import android.graphics.Bitmap
import android.widget.RemoteViews
import androidx.core.graphics.drawable.toBitmap
import coil.imageLoader
import coil.request.ImageRequest
import coil.request.SuccessResult
import uniffi.digida_mobile.PlayState
import uniffi.digida_mobile.Status

/**
 * The home-screen widget: the playing song's cover, title and artist, with previous, play and
 * next. The playback service redraws it on every change; the buttons start that service, which
 * drives the queue the way the notification's buttons do.
 */
class NowPlayingWidget : AppWidgetProvider() {
    override fun onUpdate(context: Context, manager: AppWidgetManager, ids: IntArray) {
        val status = (context.applicationContext as DigidaApp).library.status.value
        manager.updateAppWidget(ids, views(context, status, null))
    }

    companion object {
        /** The cover's edge in pixels, small enough for a widget's update to stay light. */
        private const val COVER = 192

        /** Redraws every placed widget for `status`, with `cover` loaded by [cover]. */
        fun update(context: Context, status: Status, cover: Bitmap?) {
            val manager = AppWidgetManager.getInstance(context)
            val ids = manager.getAppWidgetIds(ComponentName(context, NowPlayingWidget::class.java))
            if (ids.isEmpty()) return
            manager.updateAppWidget(ids, views(context, status, cover))
        }

        /** The cover at `url` as a small bitmap, or none when it cannot be loaded. */
        suspend fun cover(context: Context, url: String?): Bitmap? {
            if (url.isNullOrEmpty()) return null
            val request = ImageRequest.Builder(context).data(url).size(COVER).allowHardware(false).build()
            val result = context.imageLoader.execute(request) as? SuccessResult ?: return null
            return result.drawable.toBitmap(COVER, COVER)
        }

        private fun views(context: Context, status: Status, cover: Bitmap?): RemoteViews {
            val playing = status.state == PlayState.PLAYING
            return RemoteViews(context.packageName, R.layout.widget_now_playing).apply {
                setTextViewText(R.id.widget_title, status.song?.title ?: context.getString(R.string.nothing_playing))
                setTextViewText(R.id.widget_artist, status.song?.artist.orEmpty())
                if (cover != null) setImageViewBitmap(R.id.widget_cover, cover)
                else setImageViewResource(R.id.widget_cover, R.mipmap.ic_launcher)
                setImageViewResource(
                    R.id.widget_toggle,
                    if (playing) android.R.drawable.ic_media_pause else android.R.drawable.ic_media_play,
                )
                setContentDescription(
                    R.id.widget_toggle,
                    context.getString(if (playing) R.string.pause else R.string.play),
                )
                setOnClickPendingIntent(R.id.widget_previous, PlaybackService.action(context, PlaybackService.ACTION_PREVIOUS))
                setOnClickPendingIntent(R.id.widget_toggle, PlaybackService.action(context, PlaybackService.ACTION_TOGGLE))
                setOnClickPendingIntent(R.id.widget_next, PlaybackService.action(context, PlaybackService.ACTION_NEXT))
                setOnClickPendingIntent(
                    R.id.widget_root,
                    PendingIntent.getActivity(
                        context, 0, Intent(context, MainActivity::class.java), PendingIntent.FLAG_IMMUTABLE,
                    ),
                )
            }
        }
    }
}
