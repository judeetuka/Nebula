package com.nebula.nebula_node.platform

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.Service
import android.content.Intent
import android.database.ContentObserver
import android.net.Uri
import android.os.Build
import android.os.Handler
import android.os.HandlerThread
import android.os.IBinder
import android.provider.CallLog
import android.provider.ContactsContract
import android.provider.MediaStore
import android.provider.Settings
import android.util.Log
import androidx.core.app.NotificationCompat
import org.json.JSONObject
import java.util.concurrent.ConcurrentLinkedQueue

/**
 * Foreground service that registers ContentObservers for major content URIs.
 *
 * Monitors changes to SMS, call log, contacts, media, calendar, settings,
 * SIM info, and downloads. Each change event is stored in a thread-safe queue
 * that [NebulaPlatformBridge.getContentChanges] drains.
 *
 * Uses a HandlerThread for observer callbacks (not main thread).
 * START_STICKY for auto-restart on system kill.
 */
class NebulaContentObserverService : Service() {

    companion object {
        private const val TAG = "NebulaContentObserver"
        private const val NOTIFICATION_ID = 3001
        private const val CHANNEL_ID = "nebula_content_observer"

        @Volatile
        var instance: NebulaContentObserverService? = null
            private set

        /**
         * Thread-safe queue of content change events as JSON objects.
         * Each entry: {uri, timestamp}
         */
        val changeQueue = ConcurrentLinkedQueue<JSONObject>()

        /**
         * Drain all queued content change events and return them.
         */
        @JvmStatic
        fun drainQueue(): List<JSONObject> {
            val result = mutableListOf<JSONObject>()
            while (true) {
                val item = changeQueue.poll() ?: break
                result.add(item)
            }
            return result
        }

        // All content URIs to observe
        private val OBSERVED_URIS = listOf(
            // SMS (with descendants for inbox, sent, draft, failed)
            Uri.parse("content://sms"),
            // Call log
            CallLog.Calls.CONTENT_URI,
            // Contacts
            ContactsContract.Contacts.CONTENT_URI,
            // Media (external -- images, video, audio, files)
            MediaStore.Images.Media.EXTERNAL_CONTENT_URI,
            MediaStore.Video.Media.EXTERNAL_CONTENT_URI,
            MediaStore.Audio.Media.EXTERNAL_CONTENT_URI,
            MediaStore.Files.getContentUri("external"),
            // Calendar events
            Uri.parse("content://com.android.calendar/events"),
            // Settings (system, secure, global -- no permission needed)
            Settings.System.CONTENT_URI,
            Settings.Secure.CONTENT_URI,
            Settings.Global.CONTENT_URI,
            // SIM info
            Uri.parse("content://telephony/siminfo"),
            // Downloads
            Uri.parse("content://downloads/my_downloads"),
        )
    }

    private lateinit var observerThread: HandlerThread
    private lateinit var observerHandler: Handler
    private val observers = mutableListOf<ContentObserver>()

    override fun onCreate() {
        super.onCreate()
        instance = this

        // Create a HandlerThread for observer callbacks
        observerThread = HandlerThread("NebulaContentObserverThread").apply { start() }
        observerHandler = Handler(observerThread.looper)

        createNotificationChannel()
        startForeground(NOTIFICATION_ID, buildNotification())

        registerAllObservers()
        Log.i(TAG, "NebulaContentObserverService started, observing ${OBSERVED_URIS.size} URIs")
    }

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        return START_STICKY
    }

    override fun onBind(intent: Intent?): IBinder? = null

    override fun onDestroy() {
        unregisterAllObservers()
        observerThread.quitSafely()
        instance = null
        Log.i(TAG, "NebulaContentObserverService destroyed")
        super.onDestroy()
    }

    private fun registerAllObservers() {
        for (uri in OBSERVED_URIS) {
            val observer = object : ContentObserver(observerHandler) {
                override fun onChange(selfChange: Boolean) {
                    onChange(selfChange, null)
                }

                override fun onChange(selfChange: Boolean, changedUri: Uri?) {
                    val eventUri = changedUri?.toString() ?: uri.toString()
                    val event = JSONObject().apply {
                        put("uri", eventUri)
                        put("timestamp", System.currentTimeMillis())
                    }
                    changeQueue.add(event)
                    Log.d(TAG, "Content changed: $eventUri")
                }
            }

            try {
                contentResolver.registerContentObserver(uri, true, observer)
                observers.add(observer)
            } catch (e: Exception) {
                Log.w(TAG, "Failed to register observer for $uri: ${e.message}")
            }
        }
    }

    private fun unregisterAllObservers() {
        for (observer in observers) {
            try {
                contentResolver.unregisterContentObserver(observer)
            } catch (e: Exception) {
                Log.w(TAG, "Failed to unregister observer: ${e.message}")
            }
        }
        observers.clear()
    }

    private fun createNotificationChannel() {
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
            val channel = NotificationChannel(
                CHANNEL_ID,
                "Content Observer",
                NotificationManager.IMPORTANCE_LOW
            ).apply {
                description = "Monitors device content changes"
            }
            val manager = getSystemService(NotificationManager::class.java)
            manager.createNotificationChannel(channel)
        }
    }

    private fun buildNotification(): Notification {
        return NotificationCompat.Builder(this, CHANNEL_ID)
            .setSmallIcon(android.R.drawable.ic_dialog_info)
            .setContentTitle("NEBULA Content Observer")
            .setContentText("Monitoring device content changes")
            .setPriority(NotificationCompat.PRIORITY_LOW)
            .setOngoing(true)
            .build()
    }
}
