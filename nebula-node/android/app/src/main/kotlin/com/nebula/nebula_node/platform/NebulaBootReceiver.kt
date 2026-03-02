package com.nebula.nebula_node.platform

import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.os.Build
import android.util.Log

/**
 * Receives BOOT_COMPLETED broadcast to restart essential services.
 *
 * Starts the [NebulaContentObserverService] so content monitoring resumes
 * immediately after reboot. The Flutter app's foreground task plugin handles
 * restarting the main engine and MQTT connection.
 */
class NebulaBootReceiver : BroadcastReceiver() {

    companion object {
        private const val TAG = "NebulaBootReceiver"
    }

    override fun onReceive(context: Context, intent: Intent) {
        if (intent.action != Intent.ACTION_BOOT_COMPLETED) return

        Log.i(TAG, "Boot completed, starting NEBULA services")

        // Start the ContentObserverService
        val observerIntent = Intent(context, NebulaContentObserverService::class.java)
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
            context.startForegroundService(observerIntent)
        } else {
            context.startService(observerIntent)
        }

        // Start the foreground service
        val foregroundIntent = Intent(context, NebulaForegroundService::class.java)
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
            context.startForegroundService(foregroundIntent)
        } else {
            context.startService(foregroundIntent)
        }
    }
}
