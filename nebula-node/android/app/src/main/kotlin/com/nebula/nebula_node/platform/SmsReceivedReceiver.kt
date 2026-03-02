package com.nebula.nebula_node.platform

import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.os.Build
import android.telephony.SmsMessage
import android.util.Log
import org.json.JSONObject
import java.util.concurrent.ConcurrentLinkedQueue

/**
 * Manifest-registered BroadcastReceiver for incoming SMS.
 *
 * Intercepts SMS_RECEIVED at priority 999, extracts message metadata,
 * and stores it in a static queue for NebulaPlatformBridge retrieval.
 *
 * Detects Flash SMS (Class 0) messages and aborts the broadcast to prevent
 * the system from displaying the flash SMS dialog.
 *
 * Must be registered in AndroidManifest.xml:
 * <receiver android:name=".platform.SmsReceivedReceiver"
 *     android:permission="android.permission.BROADCAST_SMS"
 *     android:exported="true">
 *     <intent-filter android:priority="999">
 *         <action android:name="android.provider.Telephony.SMS_RECEIVED"/>
 *     </intent-filter>
 * </receiver>
 */
class SmsReceivedReceiver : BroadcastReceiver() {

    companion object {
        private const val TAG = "SmsReceivedReceiver"

        /**
         * Thread-safe queue of received SMS messages as JSON strings.
         * Drained by [NebulaPlatformBridge.getReceivedSms].
         */
        val smsQueue = ConcurrentLinkedQueue<JSONObject>()

        /**
         * Drain all queued SMS messages and return them.
         * After this call the queue is empty.
         */
        @JvmStatic
        fun drainQueue(): List<JSONObject> {
            val result = mutableListOf<JSONObject>()
            while (true) {
                val item = smsQueue.poll() ?: break
                result.add(item)
            }
            return result
        }

        /**
         * Clear all queued SMS messages without returning them.
         */
        @JvmStatic
        fun clearQueue() {
            smsQueue.clear()
        }
    }

    override fun onReceive(context: Context, intent: Intent) {
        if (intent.action != "android.provider.Telephony.SMS_RECEIVED") return

        val bundle = intent.extras ?: return
        val pdus = bundle.get("pdus") as? Array<*> ?: return
        val format = bundle.getString("format", "3gpp")

        // Get SIM slot index (0 or 1)
        val simSlot = intent.getIntExtra("android.telephony.extra.SLOT_INDEX", 0)

        // Reconstruct full message from PDU parts
        val messages = pdus.mapNotNull { pdu ->
            if (pdu is ByteArray) {
                SmsMessage.createFromPdu(pdu, format)
            } else null
        }

        if (messages.isEmpty()) return

        val sender = messages.first().displayOriginatingAddress ?: return
        val body = messages.joinToString("") { it.displayMessageBody ?: "" }

        if (body.isEmpty()) return

        // Detect flash SMS (Class 0) and carrier notifications
        val isFlash = messages.first().messageClass == SmsMessage.MessageClass.CLASS_0
        val isPhoneNumber = sender.startsWith("+") || sender.all { it.isDigit() }

        // Abort flash SMS broadcast to prevent the system from showing the dialog.
        // Our receiver has priority 999, so we intercept before the default SMS app.
        if (isFlash && isOrderedBroadcast) {
            abortBroadcast()
            Log.d(TAG, "Flash SMS aborted (prevented system dialog)")
        }

        // Get subscription ID for more accurate SIM identification
        val subscriptionId = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.LOLLIPOP_MR1) {
            bundle.getInt("subscription", -1).let { if (it == -1) null else it }
        } else null

        // Build the SMS JSON object and enqueue
        val smsObj = JSONObject().apply {
            put("from", sender)
            put("body", body)
            put("simSlot", simSlot)
            put("subscriptionId", subscriptionId ?: JSONObject.NULL)
            put("isFlash", isFlash)
            put("isCarrier", !isPhoneNumber)
            put("timestamp", System.currentTimeMillis())
        }

        smsQueue.add(smsObj)
        Log.d(TAG, "SMS queued from $sender (${body.length}ch, flash=$isFlash, sim=$simSlot)")
    }
}
