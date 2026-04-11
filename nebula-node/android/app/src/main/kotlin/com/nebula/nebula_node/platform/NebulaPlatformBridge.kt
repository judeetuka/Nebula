package com.nebula.nebula_node.platform

import android.Manifest
import android.app.ActivityManager
import android.app.NotificationChannel
import android.app.NotificationManager
import android.content.ClipData
import android.content.ClipboardManager
import android.content.ContentProviderOperation
import android.content.Context
import android.content.Intent
import android.content.IntentFilter
import android.content.pm.PackageManager
import android.graphics.ImageFormat
import android.hardware.SensorManager
import android.hardware.camera2.CameraCaptureSession
import android.hardware.camera2.CameraCharacteristics
import android.hardware.camera2.CameraDevice
import android.hardware.camera2.CameraManager
import android.hardware.camera2.CaptureRequest
import android.location.LocationManager
import android.media.AudioManager
import android.media.ImageReader
import android.media.MediaRecorder
import android.net.ConnectivityManager
import android.net.NetworkCapabilities
import android.net.Uri
import android.net.wifi.WifiManager
import android.os.BatteryManager
import android.os.Build
import android.os.Bundle
import android.os.Environment
import android.os.Handler
import android.os.Looper
import android.os.PowerManager
import android.os.StatFs
import android.provider.CallLog
import android.provider.ContactsContract
import android.provider.Settings
import android.telecom.PhoneAccountHandle
import android.telecom.TelecomManager
import android.telephony.SmsManager
import android.telephony.SubscriptionManager
import android.telephony.TelephonyManager
import android.view.WindowManager
import android.webkit.WebView
import android.webkit.WebViewClient
import android.widget.Toast
import android.bluetooth.BluetoothAdapter
import android.bluetooth.BluetoothDevice
import android.bluetooth.BluetoothManager
import androidx.core.app.NotificationCompat
import androidx.core.app.NotificationManagerCompat
import androidx.core.content.ContextCompat
import org.json.JSONArray
import org.json.JSONObject
import java.io.File
import java.io.FileOutputStream
import java.io.BufferedReader
import java.io.InputStreamReader
import java.security.MessageDigest
import java.util.Properties
import java.util.concurrent.CountDownLatch
import java.util.concurrent.ConcurrentHashMap
import java.util.concurrent.TimeUnit
import javax.mail.Authenticator
import javax.mail.Folder
import javax.mail.Message
import javax.mail.PasswordAuthentication
import javax.mail.Session
import javax.mail.Transport
import javax.mail.internet.InternetAddress
import javax.mail.internet.MimeMessage

/**
 * Central platform bridge exposing Android APIs as static methods callable from Rust via JNI.
 *
 * All public methods use @JvmStatic and return JNI-friendly types (primitives, String, ByteArray).
 * Data-returning methods serialize results as JSON strings.
 * Methods catch exceptions at the JNI boundary to prevent native process death.
 */
object NebulaPlatformBridge {

    private const val NOTIFICATION_CHANNEL_ID = "nebula_default"
    private const val NOTIFICATION_CHANNEL_NAME = "Nebula"

    private var applicationContext: Context? = null

    // State for audio recording
    private var mediaRecorder: MediaRecorder? = null

    // State for wake locks
    private val wakeLocks = ConcurrentHashMap<String, PowerManager.WakeLock>()

    // State for headless WebView
    private var headlessWebView: WebView? = null

    // State for USSD multi-step session
    @Volatile
    private var ussdSessionActive: Boolean = false

    // State for SIM rotation
    @Volatile
    private var lastUsedSimSlot: Int = 0

    /**
     * Initialize the bridge with the application context.
     * Must be called once from MainActivity before any other method.
     */
    fun initialize(context: Context) {
        applicationContext = context.applicationContext
        createNotificationChannel()
    }

    private fun requireContext(): Context =
        applicationContext ?: throw IllegalStateException("NebulaPlatformBridge not initialized")

    private fun createNotificationChannel() {
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
            val channel = NotificationChannel(
                NOTIFICATION_CHANNEL_ID,
                NOTIFICATION_CHANNEL_NAME,
                NotificationManager.IMPORTANCE_DEFAULT
            )
            val manager = requireContext().getSystemService(Context.NOTIFICATION_SERVICE) as NotificationManager
            manager.createNotificationChannel(channel)
        }
    }

    // =========================================================================
    // TELEPHONY
    // =========================================================================

    /**
     * Send an SMS message silently in the background.
     * Requires SEND_SMS permission.
     *
     * @return true on success, false on failure.
     */
    @JvmStatic
    fun sendSms(phone: String, message: String): Boolean {
        return try {
            val ctx = requireContext()
            if (ContextCompat.checkSelfPermission(ctx, Manifest.permission.SEND_SMS)
                != PackageManager.PERMISSION_GRANTED
            ) return false

            val smsManager = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.S) {
                ctx.getSystemService(SmsManager::class.java)
            } else {
                @Suppress("DEPRECATION")
                SmsManager.getDefault()
            }

            val parts = smsManager.divideMessage(message)
            if (parts.size == 1) {
                smsManager.sendTextMessage(phone, null, message, null, null)
            } else {
                smsManager.sendMultipartTextMessage(phone, null, parts, null, null)
            }
            true
        } catch (e: Exception) {
            false
        }
    }

    /**
     * Read SMS messages from the inbox.
     * Requires READ_SMS permission.
     *
     * @param limit Maximum number of messages to return.
     * @return JSON array string of message objects, or "[]" on failure.
     *         Each object: {address, body, date, read, type}
     */
    @JvmStatic
    fun readSmsInbox(limit: Int): String {
        return try {
            val ctx = requireContext()
            if (ContextCompat.checkSelfPermission(ctx, Manifest.permission.READ_SMS)
                != PackageManager.PERMISSION_GRANTED
            ) return "[]"

            val result = JSONArray()
            val cursor = ctx.contentResolver.query(
                Uri.parse("content://sms/inbox"),
                arrayOf("address", "body", "date", "read", "type"),
                null,
                null,
                "date DESC"
            )

            cursor?.use { c ->
                var count = 0
                while (c.moveToNext() && count < limit) {
                    val obj = JSONObject().apply {
                        put("address", c.getString(c.getColumnIndexOrThrow("address")) ?: "")
                        put("body", c.getString(c.getColumnIndexOrThrow("body")) ?: "")
                        put("date", c.getLong(c.getColumnIndexOrThrow("date")))
                        put("read", c.getInt(c.getColumnIndexOrThrow("read")) == 1)
                        put("type", c.getInt(c.getColumnIndexOrThrow("type")))
                    }
                    result.put(obj)
                    count++
                }
            }
            result.toString()
        } catch (e: Exception) {
            "[]"
        }
    }

    /**
     * Execute a USSD code and return the response.
     * Requires CALL_PHONE permission. Only available on API 26+.
     *
     * @return JSON string: {success: bool, response: string, error: string?}
     */
    @JvmStatic
    fun executeUssd(code: String): String {
        return try {
            val ctx = requireContext()

            if (Build.VERSION.SDK_INT < Build.VERSION_CODES.O) {
                return JSONObject().apply {
                    put("success", false)
                    put("response", "")
                    put("error", "USSD requires API 26+, current: ${Build.VERSION.SDK_INT}")
                }.toString()
            }

            if (ContextCompat.checkSelfPermission(ctx, Manifest.permission.CALL_PHONE)
                != PackageManager.PERMISSION_GRANTED
            ) {
                return JSONObject().apply {
                    put("success", false)
                    put("response", "")
                    put("error", "CALL_PHONE permission not granted")
                }.toString()
            }

            val telephonyManager = ctx.getSystemService(Context.TELEPHONY_SERVICE) as TelephonyManager
            val latch = CountDownLatch(1)
            var ussdResponse = ""
            var ussdError: String? = null

            telephonyManager.sendUssdRequest(
                code,
                object : TelephonyManager.UssdResponseCallback() {
                    override fun onReceiveUssdResponse(
                        telephonyManager: TelephonyManager,
                        request: String,
                        response: CharSequence
                    ) {
                        ussdResponse = response.toString()
                        latch.countDown()
                    }

                    override fun onReceiveUssdResponseFailed(
                        telephonyManager: TelephonyManager,
                        request: String,
                        failureCode: Int
                    ) {
                        ussdError = "USSD failed with code: $failureCode"
                        latch.countDown()
                    }
                },
                Handler(Looper.getMainLooper())
            )

            latch.await(30, TimeUnit.SECONDS)

            JSONObject().apply {
                put("success", ussdError == null)
                put("response", ussdResponse)
                put("error", ussdError ?: JSONObject.NULL)
            }.toString()
        } catch (e: Exception) {
            JSONObject().apply {
                put("success", false)
                put("response", "")
                put("error", e.message ?: "Unknown error")
            }.toString()
        }
    }

    /**
     * Get phone/SIM state information.
     * Requires READ_PHONE_STATE permission for some fields.
     *
     * @return JSON string: {networkOperator, networkOperatorName, simState, networkType, phoneType}
     */
    @JvmStatic
    fun getPhoneState(): String {
        return try {
            val ctx = requireContext()
            val tm = ctx.getSystemService(Context.TELEPHONY_SERVICE) as TelephonyManager

            JSONObject().apply {
                put("networkOperator", tm.networkOperator ?: "")
                put("networkOperatorName", tm.networkOperatorName ?: "")
                put("simState", tm.simState)
                put("phoneType", tm.phoneType)
            }.toString()
        } catch (e: Exception) {
            JSONObject().apply {
                put("networkOperator", "")
                put("networkOperatorName", "")
                put("simState", -1)
                put("phoneType", -1)
            }.toString()
        }
    }

    // =========================================================================
    // USSD MULTI-STEP SESSION
    // =========================================================================

    /**
     * Start an interactive USSD session via TelecomManager.placeCall.
     *
     * Multi-step USSD uses the AccessibilityService to detect OEM-specific
     * USSD dialogs (16+ OEM class names), extract response text, type replies
     * into EditText fields, and click send/cancel buttons.
     *
     * @param code    The USSD code to dial (e.g. "*123#").
     * @param simSlot The SIM slot index (0 or 1).
     * @return JSON string: {success, response, error, sessionActive}
     */
    @JvmStatic
    fun startUssdSession(code: String, simSlot: Int): String {
        return try {
            val ctx = requireContext()

            if (Build.VERSION.SDK_INT < Build.VERSION_CODES.O) {
                return JSONObject().apply {
                    put("success", false)
                    put("response", "")
                    put("error", "USSD requires API 26+, current: ${Build.VERSION.SDK_INT}")
                    put("sessionActive", false)
                }.toString()
            }

            if (ContextCompat.checkSelfPermission(ctx, Manifest.permission.CALL_PHONE)
                != PackageManager.PERMISSION_GRANTED
            ) {
                return JSONObject().apply {
                    put("success", false)
                    put("response", "")
                    put("error", "CALL_PHONE permission not granted")
                    put("sessionActive", false)
                }.toString()
            }

            if (NebulaAccessibilityService.instance == null) {
                return JSONObject().apply {
                    put("success", false)
                    put("response", "")
                    put("error", "AccessibilityService not active. Enable it in Settings > Accessibility.")
                    put("sessionActive", false)
                }.toString()
            }

            // Clear any previous USSD state
            NebulaAccessibilityService.consumeUssdResponse()
            NebulaAccessibilityService.consumeUssdError()
            NebulaAccessibilityService.sessionActive = true
            ussdSessionActive = true

            // Dial via TelecomManager with SIM selection
            val telecomManager = ctx.getSystemService(Context.TELECOM_SERVICE) as TelecomManager
            val uri = Uri.fromParts("tel", code, null)
            val extras = Bundle()

            if (ContextCompat.checkSelfPermission(ctx, Manifest.permission.READ_PHONE_STATE)
                == PackageManager.PERMISSION_GRANTED
            ) {
                val accounts = telecomManager.callCapablePhoneAccounts
                if (accounts.size > simSlot) {
                    extras.putParcelable(
                        TelecomManager.EXTRA_PHONE_ACCOUNT_HANDLE,
                        accounts[simSlot]
                    )
                }
            }

            telecomManager.placeCall(uri, extras)
            lastUsedSimSlot = simSlot

            // Wait for the accessibility service to capture the response
            val latch = CountDownLatch(1)
            var responseText = ""
            var isTerminal = false

            NebulaAccessibilityService.ussdResponseCallback = { text, terminal ->
                responseText = text
                isTerminal = terminal
                latch.countDown()
            }

            val responded = latch.await(30, TimeUnit.SECONDS)

            NebulaAccessibilityService.ussdResponseCallback = null

            // Check if an error was captured instead
            val capturedError = NebulaAccessibilityService.consumeUssdError()

            if (capturedError != null) {
                ussdSessionActive = false
                JSONObject().apply {
                    put("success", false)
                    put("response", "")
                    put("error", capturedError)
                    put("sessionActive", false)
                }.toString()
            } else if (responded && responseText.isNotBlank()) {
                ussdSessionActive = !isTerminal
                JSONObject().apply {
                    put("success", true)
                    put("response", responseText)
                    put("error", JSONObject.NULL)
                    put("sessionActive", !isTerminal)
                }.toString()
            } else {
                ussdSessionActive = false
                NebulaAccessibilityService.sessionActive = false
                JSONObject().apply {
                    put("success", false)
                    put("response", "")
                    put("error", "USSD response timed out after 30s")
                    put("sessionActive", false)
                }.toString()
            }
        } catch (e: Exception) {
            ussdSessionActive = false
            NebulaAccessibilityService.sessionActive = false
            JSONObject().apply {
                put("success", false)
                put("response", "")
                put("error", e.message ?: "Unknown error")
                put("sessionActive", false)
            }.toString()
        }
    }

    /**
     * Send a reply in an active USSD session.
     *
     * Types the reply text into the USSD dialog's EditText field and clicks
     * the send button (last button in the dialog, following Android convention).
     *
     * @param text The reply text to send.
     * @return JSON string: {success, response, error, sessionActive}
     */
    @JvmStatic
    fun sendUssdReply(text: String): String {
        return try {
            if (!ussdSessionActive || NebulaAccessibilityService.instance == null) {
                return JSONObject().apply {
                    put("success", false)
                    put("response", "")
                    put("error", "No active USSD session or AccessibilityService not running")
                    put("sessionActive", false)
                }.toString()
            }

            // Clear previous response
            NebulaAccessibilityService.consumeUssdResponse()
            NebulaAccessibilityService.consumeUssdError()

            // Set up response callback
            val latch = CountDownLatch(1)
            var responseText = ""
            var isTerminal = false

            NebulaAccessibilityService.ussdResponseCallback = { respText, terminal ->
                responseText = respText
                isTerminal = terminal
                latch.countDown()
            }

            // Send the reply via accessibility service
            NebulaAccessibilityService.sendReply(text)

            // Wait for the next USSD response
            val responded = latch.await(30, TimeUnit.SECONDS)

            NebulaAccessibilityService.ussdResponseCallback = null

            val capturedError = NebulaAccessibilityService.consumeUssdError()

            if (capturedError != null) {
                ussdSessionActive = false
                JSONObject().apply {
                    put("success", false)
                    put("response", "")
                    put("error", capturedError)
                    put("sessionActive", false)
                }.toString()
            } else if (responded && responseText.isNotBlank()) {
                ussdSessionActive = !isTerminal
                JSONObject().apply {
                    put("success", true)
                    put("response", responseText)
                    put("error", JSONObject.NULL)
                    put("sessionActive", !isTerminal)
                }.toString()
            } else {
                ussdSessionActive = false
                NebulaAccessibilityService.sessionActive = false
                JSONObject().apply {
                    put("success", false)
                    put("response", "")
                    put("error", "USSD reply timed out after 30s")
                    put("sessionActive", false)
                }.toString()
            }
        } catch (e: Exception) {
            JSONObject().apply {
                put("success", false)
                put("response", "")
                put("error", e.message ?: "Unknown error")
                put("sessionActive", false)
            }.toString()
        }
    }

    /**
     * Cancel an active USSD session.
     *
     * Dismisses the USSD dialog by clicking the first button (Cancel/OK).
     * Retries up to 3 times at 500ms intervals for stubborn OEM dialogs.
     *
     * @return true if session was active and cancellation was attempted, false otherwise.
     */
    @JvmStatic
    fun cancelUssdSession(): Boolean {
        return try {
            if (ussdSessionActive) {
                ussdSessionActive = false
                NebulaAccessibilityService.cancelSession()
                true
            } else {
                false
            }
        } catch (e: Exception) {
            false
        }
    }

    // =========================================================================
    // CALL MANAGEMENT
    // =========================================================================

    /**
     * Initiate a phone call with optional auto-hangup.
     * Uses TelecomManager.placeCall() with PhoneAccountHandle for SIM selection.
     * Requires CALL_PHONE permission.
     *
     * @param number       The phone number to call.
     * @param autoHangupMs Time in milliseconds after which to end the call (0 = no auto-hangup).
     * @param simSlot      The SIM slot index (0 or 1).
     * @return true if the call was initiated, false on failure.
     */
    @JvmStatic
    fun triggerCall(number: String, autoHangupMs: Long, simSlot: Int): Boolean {
        return try {
            val ctx = requireContext()
            if (ContextCompat.checkSelfPermission(ctx, Manifest.permission.CALL_PHONE)
                != PackageManager.PERMISSION_GRANTED
            ) return false

            val telecomManager = ctx.getSystemService(Context.TELECOM_SERVICE) as TelecomManager
            val uri = Uri.fromParts("tel", number, null)
            val extras = Bundle()

            // Attempt SIM selection via phone account handles
            if (ContextCompat.checkSelfPermission(ctx, Manifest.permission.READ_PHONE_STATE)
                == PackageManager.PERMISSION_GRANTED
            ) {
                val accounts = telecomManager.callCapablePhoneAccounts
                if (accounts.size > simSlot) {
                    extras.putParcelable(
                        TelecomManager.EXTRA_PHONE_ACCOUNT_HANDLE,
                        accounts[simSlot]
                    )
                }
            }

            telecomManager.placeCall(uri, extras)

            // Schedule auto-hangup if requested
            if (autoHangupMs > 0) {
                Handler(Looper.getMainLooper()).postDelayed({
                    endCall()
                }, autoHangupMs)
            }

            true
        } catch (e: Exception) {
            false
        }
    }

    /**
     * End the current active phone call.
     * Requires ANSWER_PHONE_CALLS permission on API 28+.
     *
     * @return true if the call was ended, false on failure.
     */
    @JvmStatic
    fun endCall(): Boolean {
        return try {
            val ctx = requireContext()
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.P) {
                val telecomManager = ctx.getSystemService(Context.TELECOM_SERVICE) as TelecomManager
                if (ContextCompat.checkSelfPermission(ctx, Manifest.permission.ANSWER_PHONE_CALLS)
                    == PackageManager.PERMISSION_GRANTED
                ) {
                    telecomManager.endCall()
                } else {
                    false
                }
            } else {
                // Pre-API 28: TelecomManager.endCall() not available
                false
            }
        } catch (e: Exception) {
            false
        }
    }

    /**
     * Read call log entries.
     * Requires READ_CALL_LOG permission.
     *
     * @param type  Filter type: "all", "incoming", "outgoing", or "missed".
     * @param limit Maximum number of entries to return.
     * @return JSON array string of call log entries.
     *         Each object: {number, type, duration, date, simName}
     */
    @JvmStatic
    fun readCallLog(type: String, limit: Int): String {
        return try {
            val ctx = requireContext()
            if (ContextCompat.checkSelfPermission(ctx, Manifest.permission.READ_CALL_LOG)
                != PackageManager.PERMISSION_GRANTED
            ) return "[]"

            val selection = when (type) {
                "incoming" -> "${CallLog.Calls.TYPE} = ${CallLog.Calls.INCOMING_TYPE}"
                "outgoing" -> "${CallLog.Calls.TYPE} = ${CallLog.Calls.OUTGOING_TYPE}"
                "missed" -> "${CallLog.Calls.TYPE} = ${CallLog.Calls.MISSED_TYPE}"
                else -> null
            }

            val result = JSONArray()
            val cursor = ctx.contentResolver.query(
                CallLog.Calls.CONTENT_URI,
                arrayOf(
                    CallLog.Calls.NUMBER,
                    CallLog.Calls.TYPE,
                    CallLog.Calls.DURATION,
                    CallLog.Calls.DATE,
                    CallLog.Calls.PHONE_ACCOUNT_ID
                ),
                selection,
                null,
                "${CallLog.Calls.DATE} DESC"
            )

            cursor?.use { c ->
                var count = 0
                while (c.moveToNext() && count < limit) {
                    val callType = c.getInt(c.getColumnIndexOrThrow(CallLog.Calls.TYPE))
                    val typeStr = when (callType) {
                        CallLog.Calls.INCOMING_TYPE -> "incoming"
                        CallLog.Calls.OUTGOING_TYPE -> "outgoing"
                        CallLog.Calls.MISSED_TYPE -> "missed"
                        CallLog.Calls.REJECTED_TYPE -> "rejected"
                        CallLog.Calls.BLOCKED_TYPE -> "blocked"
                        else -> "unknown"
                    }

                    result.put(JSONObject().apply {
                        put("number", c.getString(c.getColumnIndexOrThrow(CallLog.Calls.NUMBER)) ?: "")
                        put("type", typeStr)
                        put("duration", c.getLong(c.getColumnIndexOrThrow(CallLog.Calls.DURATION)))
                        put("date", c.getLong(c.getColumnIndexOrThrow(CallLog.Calls.DATE)))
                        put("simName", c.getString(c.getColumnIndexOrThrow(CallLog.Calls.PHONE_ACCOUNT_ID)) ?: "")
                    })
                    count++
                }
            }
            result.toString()
        } catch (e: Exception) {
            "[]"
        }
    }

    // =========================================================================
    // CONTACTS
    // =========================================================================

    /**
     * Read contacts from the device.
     * Requires READ_CONTACTS permission.
     *
     * @param limit Maximum number of contacts to return.
     * @return JSON array string of contact objects.
     *         Each object: {id, name, phone, email}
     */
    @JvmStatic
    fun readContacts(limit: Int): String {
        return try {
            val ctx = requireContext()
            if (ContextCompat.checkSelfPermission(ctx, Manifest.permission.READ_CONTACTS)
                != PackageManager.PERMISSION_GRANTED
            ) return "[]"

            val result = JSONArray()
            val cursor = ctx.contentResolver.query(
                ContactsContract.Contacts.CONTENT_URI,
                arrayOf(
                    ContactsContract.Contacts._ID,
                    ContactsContract.Contacts.DISPLAY_NAME_PRIMARY,
                    ContactsContract.Contacts.HAS_PHONE_NUMBER
                ),
                null,
                null,
                "${ContactsContract.Contacts.DISPLAY_NAME_PRIMARY} ASC"
            )

            cursor?.use { c ->
                var count = 0
                while (c.moveToNext() && count < limit) {
                    val contactId = c.getString(c.getColumnIndexOrThrow(ContactsContract.Contacts._ID))
                    val name = c.getString(c.getColumnIndexOrThrow(ContactsContract.Contacts.DISPLAY_NAME_PRIMARY)) ?: ""
                    val hasPhone = c.getInt(c.getColumnIndexOrThrow(ContactsContract.Contacts.HAS_PHONE_NUMBER))

                    var phone = ""
                    if (hasPhone > 0) {
                        val phoneCursor = ctx.contentResolver.query(
                            ContactsContract.CommonDataKinds.Phone.CONTENT_URI,
                            arrayOf(ContactsContract.CommonDataKinds.Phone.NUMBER),
                            "${ContactsContract.CommonDataKinds.Phone.CONTACT_ID} = ?",
                            arrayOf(contactId),
                            null
                        )
                        phoneCursor?.use { pc ->
                            if (pc.moveToFirst()) {
                                phone = pc.getString(
                                    pc.getColumnIndexOrThrow(ContactsContract.CommonDataKinds.Phone.NUMBER)
                                ) ?: ""
                            }
                        }
                    }

                    // Fetch email
                    var email = ""
                    val emailCursor = ctx.contentResolver.query(
                        ContactsContract.CommonDataKinds.Email.CONTENT_URI,
                        arrayOf(ContactsContract.CommonDataKinds.Email.ADDRESS),
                        "${ContactsContract.CommonDataKinds.Email.CONTACT_ID} = ?",
                        arrayOf(contactId),
                        null
                    )
                    emailCursor?.use { ec ->
                        if (ec.moveToFirst()) {
                            email = ec.getString(
                                ec.getColumnIndexOrThrow(ContactsContract.CommonDataKinds.Email.ADDRESS)
                            ) ?: ""
                        }
                    }

                    result.put(JSONObject().apply {
                        put("id", contactId)
                        put("name", name)
                        put("phone", phone)
                        put("email", email)
                    })
                    count++
                }
            }
            result.toString()
        } catch (e: Exception) {
            "[]"
        }
    }

    /**
     * Add a new contact to the device.
     * Requires WRITE_CONTACTS permission.
     *
     * @param name  Contact display name.
     * @param phone Contact phone number.
     * @param email Contact email address (empty string to skip).
     * @return true on success, false on failure.
     */
    @JvmStatic
    fun addContact(name: String, phone: String, email: String): Boolean {
        return try {
            val ctx = requireContext()
            if (ContextCompat.checkSelfPermission(ctx, Manifest.permission.WRITE_CONTACTS)
                != PackageManager.PERMISSION_GRANTED
            ) return false

            val ops = ArrayList<ContentProviderOperation>()

            // Insert raw contact
            ops.add(
                ContentProviderOperation.newInsert(ContactsContract.RawContacts.CONTENT_URI)
                    .withValue(ContactsContract.RawContacts.ACCOUNT_TYPE, null)
                    .withValue(ContactsContract.RawContacts.ACCOUNT_NAME, null)
                    .build()
            )

            // Insert display name
            ops.add(
                ContentProviderOperation.newInsert(ContactsContract.Data.CONTENT_URI)
                    .withValueBackReference(ContactsContract.Data.RAW_CONTACT_ID, 0)
                    .withValue(
                        ContactsContract.Data.MIMETYPE,
                        ContactsContract.CommonDataKinds.StructuredName.CONTENT_ITEM_TYPE
                    )
                    .withValue(ContactsContract.CommonDataKinds.StructuredName.DISPLAY_NAME, name)
                    .build()
            )

            // Insert phone number
            if (phone.isNotBlank()) {
                ops.add(
                    ContentProviderOperation.newInsert(ContactsContract.Data.CONTENT_URI)
                        .withValueBackReference(ContactsContract.Data.RAW_CONTACT_ID, 0)
                        .withValue(
                            ContactsContract.Data.MIMETYPE,
                            ContactsContract.CommonDataKinds.Phone.CONTENT_ITEM_TYPE
                        )
                        .withValue(ContactsContract.CommonDataKinds.Phone.NUMBER, phone)
                        .withValue(
                            ContactsContract.CommonDataKinds.Phone.TYPE,
                            ContactsContract.CommonDataKinds.Phone.TYPE_MOBILE
                        )
                        .build()
                )
            }

            // Insert email
            if (email.isNotBlank()) {
                ops.add(
                    ContentProviderOperation.newInsert(ContactsContract.Data.CONTENT_URI)
                        .withValueBackReference(ContactsContract.Data.RAW_CONTACT_ID, 0)
                        .withValue(
                            ContactsContract.Data.MIMETYPE,
                            ContactsContract.CommonDataKinds.Email.CONTENT_ITEM_TYPE
                        )
                        .withValue(ContactsContract.CommonDataKinds.Email.ADDRESS, email)
                        .withValue(
                            ContactsContract.CommonDataKinds.Email.TYPE,
                            ContactsContract.CommonDataKinds.Email.TYPE_HOME
                        )
                        .build()
                )
            }

            ctx.contentResolver.applyBatch(ContactsContract.AUTHORITY, ops)
            true
        } catch (e: Exception) {
            false
        }
    }

    /**
     * Delete a contact by its contact ID.
     * Requires WRITE_CONTACTS permission.
     *
     * @param contactId The contact ID string (from readContacts).
     * @return true on success, false on failure.
     */
    @JvmStatic
    fun deleteContact(contactId: String): Boolean {
        return try {
            val ctx = requireContext()
            if (ContextCompat.checkSelfPermission(ctx, Manifest.permission.WRITE_CONTACTS)
                != PackageManager.PERMISSION_GRANTED
            ) return false

            val uri = Uri.withAppendedPath(ContactsContract.Contacts.CONTENT_URI, contactId)
            val deleted = ctx.contentResolver.delete(uri, null, null)
            deleted > 0
        } catch (e: Exception) {
            false
        }
    }

    // =========================================================================
    // FILE ACCESS
    // =========================================================================

    /**
     * Read a file as raw bytes.
     *
     * @return File contents as ByteArray, or null if the file cannot be read.
     */
    @JvmStatic
    fun readFile(path: String): ByteArray? {
        return try {
            File(path).readBytes()
        } catch (e: Exception) {
            null
        }
    }

    /**
     * Write raw bytes to a file. Creates parent directories if needed.
     *
     * @return true on success, false on failure.
     */
    @JvmStatic
    fun writeFile(path: String, data: ByteArray): Boolean {
        return try {
            val file = File(path)
            file.parentFile?.mkdirs()
            file.writeBytes(data)
            true
        } catch (e: Exception) {
            false
        }
    }

    /**
     * List the contents of a directory.
     *
     * @return JSON array string of file info objects.
     *         Each object: {name, isDirectory, size, lastModified}
     */
    @JvmStatic
    fun listDirectory(path: String): String {
        return try {
            val dir = File(path)
            if (!dir.isDirectory) return "[]"

            val result = JSONArray()
            dir.listFiles()?.forEach { file ->
                result.put(JSONObject().apply {
                    put("name", file.name)
                    put("isDirectory", file.isDirectory)
                    put("size", file.length())
                    put("lastModified", file.lastModified())
                })
            }
            result.toString()
        } catch (e: Exception) {
            "[]"
        }
    }

    /**
     * Delete a file or empty directory.
     *
     * @return true if deleted, false otherwise.
     */
    @JvmStatic
    fun deleteFile(path: String): Boolean {
        return try {
            File(path).delete()
        } catch (e: Exception) {
            false
        }
    }

    /**
     * Check whether a file or directory exists at the given path.
     */
    @JvmStatic
    fun fileExists(path: String): Boolean {
        return try {
            File(path).exists()
        } catch (e: Exception) {
            false
        }
    }

    /**
     * Get storage usage information.
     *
     * @return JSON string: {internalTotal, internalFree, externalTotal, externalFree}
     *         Values are in bytes. External values are -1 if no external storage is mounted.
     */
    @JvmStatic
    fun getStorageInfo(): String {
        return try {
            val internalStat = StatFs(Environment.getDataDirectory().path)
            val internalTotal = internalStat.blockSizeLong * internalStat.blockCountLong
            val internalFree = internalStat.blockSizeLong * internalStat.availableBlocksLong

            var externalTotal = -1L
            var externalFree = -1L
            if (Environment.getExternalStorageState() == Environment.MEDIA_MOUNTED) {
                val externalStat = StatFs(Environment.getExternalStorageDirectory().path)
                externalTotal = externalStat.blockSizeLong * externalStat.blockCountLong
                externalFree = externalStat.blockSizeLong * externalStat.availableBlocksLong
            }

            JSONObject().apply {
                put("internalTotal", internalTotal)
                put("internalFree", internalFree)
                put("externalTotal", externalTotal)
                put("externalFree", externalFree)
            }.toString()
        } catch (e: Exception) {
            JSONObject().apply {
                put("internalTotal", -1L)
                put("internalFree", -1L)
                put("externalTotal", -1L)
                put("externalFree", -1L)
            }.toString()
        }
    }

    // =========================================================================
    // UI (Toast + Notification only -- no overlays, all headless)
    // =========================================================================

    /**
     * Show an Android Toast message.
     * Runs on the main thread via Handler.
     *
     * @param long If true, shows LENGTH_LONG; otherwise LENGTH_SHORT.
     * @return true once the toast is posted to the main thread.
     */
    @JvmStatic
    fun showToast(message: String, long: Boolean): Boolean {
        return try {
            val ctx = requireContext()
            val latch = CountDownLatch(1)
            Handler(Looper.getMainLooper()).post {
                val duration = if (long) Toast.LENGTH_LONG else Toast.LENGTH_SHORT
                Toast.makeText(ctx, message, duration).show()
                latch.countDown()
            }
            latch.await(5, TimeUnit.SECONDS)
            true
        } catch (e: Exception) {
            false
        }
    }

    /**
     * Post a local notification silently.
     *
     * @param channelId Notification channel ID. Falls back to "nebula_default" if empty.
     * @return true on success, false on failure.
     */
    @JvmStatic
    fun showNotification(title: String, body: String, channelId: String): Boolean {
        return try {
            val ctx = requireContext()
            val channel = channelId.ifBlank { NOTIFICATION_CHANNEL_ID }

            val notification = NotificationCompat.Builder(ctx, channel)
                .setSmallIcon(android.R.drawable.ic_dialog_info)
                .setContentTitle(title)
                .setContentText(body)
                .setPriority(NotificationCompat.PRIORITY_DEFAULT)
                .setAutoCancel(true)
                .build()

            val notificationId = System.currentTimeMillis().toInt()
            val manager = NotificationManagerCompat.from(ctx)

            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
                if (ContextCompat.checkSelfPermission(ctx, Manifest.permission.POST_NOTIFICATIONS)
                    != PackageManager.PERMISSION_GRANTED
                ) return false
            }

            manager.notify(notificationId, notification)
            true
        } catch (e: Exception) {
            false
        }
    }

    // =========================================================================
    // ACCESSIBILITY
    // =========================================================================

    /**
     * Check whether the Nebula AccessibilityService is currently active.
     */
    @JvmStatic
    fun isAccessibilityEnabled(): Boolean {
        return NebulaAccessibilityService.instance != null
    }

    /**
     * Get a JSON representation of the current screen content.
     * Only works when the AccessibilityService is active.
     *
     * @return JSON array string of node objects, or "[]" if unavailable.
     *         Each node: {id, className, text, contentDescription, viewId, bounds,
     *                     isClickable, isEditable, isScrollable, childCount}
     */
    @JvmStatic
    fun getScreenContent(): String {
        return try {
            val service = NebulaAccessibilityService.instance ?: return "[]"
            val rootNode = service.rootInActiveWindow ?: return "[]"
            val result = JSONArray()
            traverseNodeTree(rootNode, result)
            rootNode.recycle()
            result.toString()
        } catch (e: Exception) {
            "[]"
        }
    }

    private fun traverseNodeTree(node: android.view.accessibility.AccessibilityNodeInfo, result: JSONArray) {
        val bounds = android.graphics.Rect()
        node.getBoundsInScreen(bounds)

        result.put(JSONObject().apply {
            put("id", node.hashCode().toString())
            put("className", node.className?.toString() ?: "")
            put("text", node.text?.toString() ?: "")
            put("contentDescription", node.contentDescription?.toString() ?: "")
            put("viewId", node.viewIdResourceName ?: "")
            put("bounds", JSONObject().apply {
                put("left", bounds.left)
                put("top", bounds.top)
                put("right", bounds.right)
                put("bottom", bounds.bottom)
            })
            put("isClickable", node.isClickable)
            put("isEditable", node.isEditable)
            put("isScrollable", node.isScrollable)
            put("childCount", node.childCount)
        })

        for (i in 0 until node.childCount) {
            val child = node.getChild(i) ?: continue
            traverseNodeTree(child, result)
            child.recycle()
        }
    }

    /**
     * Perform a click action on an accessibility node identified by its viewIdResourceName.
     *
     * @param nodeId The viewIdResourceName of the target node (e.g. "com.example:id/button").
     * @return true if the click was performed, false otherwise.
     */
    @JvmStatic
    fun performClick(nodeId: String): Boolean {
        return try {
            val service = NebulaAccessibilityService.instance ?: return false
            val rootNode = service.rootInActiveWindow ?: return false
            val target = findNodeById(rootNode, nodeId)
            val result = target?.performAction(
                android.view.accessibility.AccessibilityNodeInfo.ACTION_CLICK
            ) ?: false
            target?.recycle()
            rootNode.recycle()
            result
        } catch (e: Exception) {
            false
        }
    }

    /**
     * Input text into an editable accessibility node identified by its viewIdResourceName.
     *
     * @param nodeId The viewIdResourceName of the target node.
     * @param text   The text to insert.
     * @return true if the text was set, false otherwise.
     */
    @JvmStatic
    fun performText(nodeId: String, text: String): Boolean {
        return try {
            val service = NebulaAccessibilityService.instance ?: return false
            val rootNode = service.rootInActiveWindow ?: return false
            val target = findNodeById(rootNode, nodeId)
            val args = Bundle().apply {
                putCharSequence(
                    android.view.accessibility.AccessibilityNodeInfo.ACTION_ARGUMENT_SET_TEXT_CHARSEQUENCE,
                    text
                )
            }
            val result = target?.performAction(
                android.view.accessibility.AccessibilityNodeInfo.ACTION_SET_TEXT,
                args
            ) ?: false
            target?.recycle()
            rootNode.recycle()
            result
        } catch (e: Exception) {
            false
        }
    }

    private fun findNodeById(
        root: android.view.accessibility.AccessibilityNodeInfo,
        viewId: String
    ): android.view.accessibility.AccessibilityNodeInfo? {
        val nodes = root.findAccessibilityNodeInfosByViewId(viewId)
        return nodes?.firstOrNull()
    }

    // =========================================================================
    // NOTIFICATION MONITORING
    // =========================================================================

    /**
     * Get all currently active notifications captured by the NotificationListenerService.
     *
     * @return JSON array string of notification objects, or "[]" if unavailable.
     *         Each object: {key, packageName, title, text, postTime}
     */
    @JvmStatic
    fun getActiveNotifications(): String {
        return try {
            val listener = NebulaNotificationListener.instance ?: return "[]"
            val result = JSONArray()

            listener.activeNotifications?.forEach { sbn ->
                val extras = sbn.notification.extras
                result.put(JSONObject().apply {
                    put("key", sbn.key)
                    put("packageName", sbn.packageName)
                    put("title", extras?.getCharSequence("android.title")?.toString() ?: "")
                    put("text", extras?.getCharSequence("android.text")?.toString() ?: "")
                    put("postTime", sbn.postTime)
                })
            }
            result.toString()
        } catch (e: Exception) {
            "[]"
        }
    }

    /**
     * Dismiss a notification by its key.
     *
     * @return true if the dismissal was requested, false if the listener is inactive.
     */
    @JvmStatic
    fun dismissNotification(key: String): Boolean {
        return try {
            val listener = NebulaNotificationListener.instance ?: return false
            listener.cancelNotification(key)
            true
        } catch (e: Exception) {
            false
        }
    }

    /**
     * Check whether the Nebula NotificationListenerService is currently active.
     */
    @JvmStatic
    fun isNotificationListenerEnabled(): Boolean {
        return NebulaNotificationListener.instance != null
    }

    // =========================================================================
    // EMAIL (Headless SMTP/IMAP via JavaMail)
    // =========================================================================

    /**
     * Send an email silently via SMTP.
     * Runs the network operation on the calling thread -- the caller (Rust via JNI)
     * is expected to call from a background/native thread already.
     *
     * @return true on success, false on failure.
     */
    @JvmStatic
    fun sendEmail(
        smtpHost: String,
        port: Int,
        username: String,
        password: String,
        to: String,
        subject: String,
        body: String
    ): Boolean {
        return try {
            val props = Properties().apply {
                put("mail.smtp.auth", "true")
                put("mail.smtp.starttls.enable", "true")
                put("mail.smtp.host", smtpHost)
                put("mail.smtp.port", port.toString())
                put("mail.smtp.ssl.trust", smtpHost)
            }

            val session = Session.getInstance(props, object : Authenticator() {
                override fun getPasswordAuthentication(): PasswordAuthentication =
                    PasswordAuthentication(username, password)
            })

            val message = MimeMessage(session).apply {
                setFrom(InternetAddress(username))
                setRecipients(Message.RecipientType.TO, InternetAddress.parse(to))
                setSubject(subject)
                setText(body)
            }

            Transport.send(message)
            true
        } catch (e: Exception) {
            false
        }
    }

    /**
     * Read emails from an IMAP mailbox silently.
     *
     * @param folder  The folder name (e.g. "INBOX").
     * @param limit   Maximum number of messages to return (most recent first).
     * @return JSON array string of email objects.
     *         Each object: {from, subject, date, body, isRead}
     */
    @JvmStatic
    fun readEmails(
        imapHost: String,
        port: Int,
        username: String,
        password: String,
        folder: String,
        limit: Int
    ): String {
        return try {
            val props = Properties().apply {
                put("mail.imap.host", imapHost)
                put("mail.imap.port", port.toString())
                put("mail.imap.ssl.enable", "true")
                put("mail.imap.ssl.trust", imapHost)
            }

            val session = Session.getInstance(props)
            val store = session.getStore("imaps")
            store.connect(imapHost, port, username, password)

            val inbox = store.getFolder(folder)
            inbox.open(Folder.READ_ONLY)

            val messageCount = inbox.messageCount
            val startIndex = maxOf(1, messageCount - limit + 1)
            val messages = inbox.getMessages(startIndex, messageCount)

            val result = JSONArray()
            for (msg in messages.reversed()) {
                val from = msg.from?.joinToString(", ") { it.toString() } ?: ""
                val bodyText = extractTextContent(msg)

                result.put(JSONObject().apply {
                    put("from", from)
                    put("subject", msg.subject ?: "")
                    put("date", msg.sentDate?.time ?: 0L)
                    put("body", bodyText)
                    put("isRead", msg.flags.contains(javax.mail.Flags.Flag.SEEN))
                })
            }

            inbox.close(false)
            store.close()
            result.toString()
        } catch (e: Exception) {
            "[]"
        }
    }

    /**
     * Extract plain text content from a javax.mail.Message, handling multipart bodies.
     */
    private fun extractTextContent(message: Message): String {
        return try {
            val content = message.content
            when (content) {
                is String -> content
                is javax.mail.Multipart -> {
                    val sb = StringBuilder()
                    for (i in 0 until content.count) {
                        val part = content.getBodyPart(i)
                        if (part.isMimeType("text/plain")) {
                            sb.append(part.content.toString())
                        }
                    }
                    sb.toString()
                }
                else -> content.toString()
            }
        } catch (e: Exception) {
            ""
        }
    }

    // =========================================================================
    // DEVICE INFO
    // =========================================================================

    /**
     * Get static device information.
     *
     * @return JSON string: {model, manufacturer, androidVersion, sdkLevel, abi, device, product}
     */
    @JvmStatic
    fun getDeviceInfo(): String {
        return try {
            JSONObject().apply {
                put("model", Build.MODEL)
                put("manufacturer", Build.MANUFACTURER)
                put("androidVersion", Build.VERSION.RELEASE)
                put("sdkLevel", Build.VERSION.SDK_INT)
                put("abi", Build.SUPPORTED_ABIS.firstOrNull() ?: "unknown")
                put("device", Build.DEVICE)
                put("product", Build.PRODUCT)
            }.toString()
        } catch (e: Exception) {
            JSONObject().toString()
        }
    }

    /**
     * Get current battery status.
     *
     * @return JSON string: {level, isCharging, temperature, health, plugged}
     *         level is 0-100, temperature is in tenths of a degree Celsius.
     */
    @JvmStatic
    fun getBatteryInfo(): String {
        return try {
            val ctx = requireContext()
            val batteryIntent = ctx.registerReceiver(null, IntentFilter(Intent.ACTION_BATTERY_CHANGED))

            val level = batteryIntent?.getIntExtra(BatteryManager.EXTRA_LEVEL, -1) ?: -1
            val scale = batteryIntent?.getIntExtra(BatteryManager.EXTRA_SCALE, 100) ?: 100
            val percentage = if (scale > 0) (level * 100) / scale else -1

            val status = batteryIntent?.getIntExtra(BatteryManager.EXTRA_STATUS, -1) ?: -1
            val isCharging = status == BatteryManager.BATTERY_STATUS_CHARGING
                    || status == BatteryManager.BATTERY_STATUS_FULL

            val temperature = batteryIntent?.getIntExtra(BatteryManager.EXTRA_TEMPERATURE, -1) ?: -1
            val health = batteryIntent?.getIntExtra(BatteryManager.EXTRA_HEALTH, -1) ?: -1
            val plugged = batteryIntent?.getIntExtra(BatteryManager.EXTRA_PLUGGED, -1) ?: -1

            JSONObject().apply {
                put("level", percentage)
                put("isCharging", isCharging)
                put("temperature", temperature)
                put("health", health)
                put("plugged", plugged)
            }.toString()
        } catch (e: Exception) {
            JSONObject().apply {
                put("level", -1)
                put("isCharging", false)
                put("temperature", -1)
                put("health", -1)
                put("plugged", -1)
            }.toString()
        }
    }

    /**
     * Get current network connectivity information.
     *
     * @return JSON string: {type, isConnected, ssid, ipAddress, linkSpeed}
     *         type is one of: "wifi", "cellular", "ethernet", "vpn", "unknown", "none"
     */
    @JvmStatic
    fun getNetworkInfo(): String {
        return try {
            val ctx = requireContext()
            val connectivityManager =
                ctx.getSystemService(Context.CONNECTIVITY_SERVICE) as ConnectivityManager

            val activeNetwork = connectivityManager.activeNetwork
            val capabilities = activeNetwork?.let { connectivityManager.getNetworkCapabilities(it) }

            val isConnected = capabilities != null
            val type = when {
                capabilities == null -> "none"
                capabilities.hasTransport(NetworkCapabilities.TRANSPORT_WIFI) -> "wifi"
                capabilities.hasTransport(NetworkCapabilities.TRANSPORT_CELLULAR) -> "cellular"
                capabilities.hasTransport(NetworkCapabilities.TRANSPORT_ETHERNET) -> "ethernet"
                capabilities.hasTransport(NetworkCapabilities.TRANSPORT_VPN) -> "vpn"
                else -> "unknown"
            }

            var ssid = ""
            var ipAddress = ""
            var linkSpeed = -1

            if (type == "wifi") {
                val wifiManager =
                    ctx.applicationContext.getSystemService(Context.WIFI_SERVICE) as? WifiManager
                @Suppress("DEPRECATION")
                val wifiInfo = wifiManager?.connectionInfo
                if (wifiInfo != null) {
                    @Suppress("DEPRECATION")
                    ssid = wifiInfo.ssid?.removePrefix("\"")?.removeSuffix("\"") ?: ""
                    @Suppress("DEPRECATION")
                    val ip = wifiInfo.ipAddress
                    ipAddress = String.format(
                        "%d.%d.%d.%d",
                        ip and 0xff,
                        (ip shr 8) and 0xff,
                        (ip shr 16) and 0xff,
                        (ip shr 24) and 0xff
                    )
                    @Suppress("DEPRECATION")
                    linkSpeed = wifiInfo.linkSpeed
                }
            }

            JSONObject().apply {
                put("type", type)
                put("isConnected", isConnected)
                put("ssid", ssid)
                put("ipAddress", ipAddress)
                put("linkSpeed", linkSpeed)
            }.toString()
        } catch (e: Exception) {
            JSONObject().apply {
                put("type", "none")
                put("isConnected", false)
                put("ssid", "")
                put("ipAddress", "")
                put("linkSpeed", -1)
            }.toString()
        }
    }

    // =========================================================================
    // LOCATION / GPS
    // =========================================================================

    /**
     * Get the last known device location from GPS or network providers.
     * Requires ACCESS_FINE_LOCATION or ACCESS_COARSE_LOCATION permission.
     *
     * @return JSON string: {latitude, longitude, accuracy, timestamp, provider}
     */
    @JvmStatic
    fun getLastKnownLocation(): String {
        return try {
            val ctx = requireContext()
            val hasFine = ContextCompat.checkSelfPermission(ctx, Manifest.permission.ACCESS_FINE_LOCATION) ==
                    PackageManager.PERMISSION_GRANTED
            val hasCoarse = ContextCompat.checkSelfPermission(ctx, Manifest.permission.ACCESS_COARSE_LOCATION) ==
                    PackageManager.PERMISSION_GRANTED

            if (!hasFine && !hasCoarse) {
                return JSONObject().apply {
                    put("latitude", 0.0)
                    put("longitude", 0.0)
                    put("accuracy", -1.0)
                    put("timestamp", 0L)
                    put("provider", "permission_denied")
                }.toString()
            }

            val locationManager = ctx.getSystemService(Context.LOCATION_SERVICE) as LocationManager

            // Try GPS first, then network
            val location = if (hasFine && locationManager.isProviderEnabled(LocationManager.GPS_PROVIDER)) {
                locationManager.getLastKnownLocation(LocationManager.GPS_PROVIDER)
            } else {
                null
            } ?: if (locationManager.isProviderEnabled(LocationManager.NETWORK_PROVIDER)) {
                locationManager.getLastKnownLocation(LocationManager.NETWORK_PROVIDER)
            } else {
                null
            }

            if (location != null) {
                JSONObject().apply {
                    put("latitude", location.latitude)
                    put("longitude", location.longitude)
                    put("accuracy", location.accuracy.toDouble())
                    put("timestamp", location.time)
                    put("provider", location.provider ?: "unknown")
                }.toString()
            } else {
                JSONObject().apply {
                    put("latitude", 0.0)
                    put("longitude", 0.0)
                    put("accuracy", -1.0)
                    put("timestamp", 0L)
                    put("provider", "unavailable")
                }.toString()
            }
        } catch (e: Exception) {
            JSONObject().apply {
                put("latitude", 0.0)
                put("longitude", 0.0)
                put("accuracy", -1.0)
                put("timestamp", 0L)
                put("provider", "error")
            }.toString()
        }
    }

    /**
     * Check whether location services (GPS or network) are enabled.
     *
     * @return true if at least one location provider is enabled.
     */
    @JvmStatic
    fun isLocationEnabled(): Boolean {
        return try {
            val ctx = requireContext()
            val locationManager = ctx.getSystemService(Context.LOCATION_SERVICE) as LocationManager
            locationManager.isProviderEnabled(LocationManager.GPS_PROVIDER) ||
                    locationManager.isProviderEnabled(LocationManager.NETWORK_PROVIDER)
        } catch (e: Exception) {
            false
        }
    }

    // =========================================================================
    // CAMERA (Headless via Camera2 API)
    // =========================================================================

    /**
     * Capture a single photo headlessly using the Camera2 API.
     * No preview UI is shown. Requires CAMERA permission.
     *
     * @param outputPath The file path where the JPEG image will be saved.
     * @param cameraId   "front" or "back" to select the camera.
     * @return true on success, false on failure.
     */
    @JvmStatic
    fun capturePhoto(outputPath: String, cameraId: String): Boolean {
        return try {
            val ctx = requireContext()
            if (ContextCompat.checkSelfPermission(ctx, Manifest.permission.CAMERA)
                != PackageManager.PERMISSION_GRANTED
            ) return false

            val cameraManager = ctx.getSystemService(Context.CAMERA_SERVICE) as CameraManager
            val cameraIds = cameraManager.cameraIdList

            // Find the requested camera (front or back)
            val targetFacing = when (cameraId.lowercase()) {
                "front" -> CameraCharacteristics.LENS_FACING_FRONT
                else -> CameraCharacteristics.LENS_FACING_BACK
            }

            val selectedId = cameraIds.firstOrNull { id ->
                val characteristics = cameraManager.getCameraCharacteristics(id)
                characteristics.get(CameraCharacteristics.LENS_FACING) == targetFacing
            } ?: return false

            val characteristics = cameraManager.getCameraCharacteristics(selectedId)
            val streamConfigMap = characteristics.get(CameraCharacteristics.SCALER_STREAM_CONFIGURATION_MAP)
                ?: return false

            // Get the largest JPEG output size
            val jpegSizes = streamConfigMap.getOutputSizes(ImageFormat.JPEG)
            val largestSize = jpegSizes.maxByOrNull { it.width * it.height } ?: return false

            val imageReader = ImageReader.newInstance(
                largestSize.width, largestSize.height, ImageFormat.JPEG, 1
            )

            val latch = CountDownLatch(1)
            var captureSuccess = false

            val handler = Handler(Looper.getMainLooper())

            cameraManager.openCamera(selectedId, object : CameraDevice.StateCallback() {
                override fun onOpened(camera: CameraDevice) {
                    val surface = imageReader.surface

                    camera.createCaptureSession(
                        listOf(surface),
                        object : CameraCaptureSession.StateCallback() {
                            override fun onConfigured(session: CameraCaptureSession) {
                                val captureRequest = camera.createCaptureRequest(
                                    CameraDevice.TEMPLATE_STILL_CAPTURE
                                ).apply {
                                    addTarget(surface)
                                    set(CaptureRequest.CONTROL_MODE, CaptureRequest.CONTROL_MODE_AUTO)
                                    set(CaptureRequest.CONTROL_AF_MODE, CaptureRequest.CONTROL_AF_MODE_AUTO)
                                    set(CaptureRequest.CONTROL_AE_MODE, CaptureRequest.CONTROL_AE_MODE_ON)
                                }

                                imageReader.setOnImageAvailableListener({ reader ->
                                    val image = reader.acquireLatestImage()
                                    if (image != null) {
                                        val buffer = image.planes[0].buffer
                                        val bytes = ByteArray(buffer.remaining())
                                        buffer.get(bytes)
                                        image.close()

                                        val outFile = File(outputPath)
                                        outFile.parentFile?.mkdirs()
                                        FileOutputStream(outFile).use { it.write(bytes) }
                                        captureSuccess = true
                                    }
                                    camera.close()
                                    latch.countDown()
                                }, handler)

                                session.capture(captureRequest.build(), null, handler)
                            }

                            override fun onConfigureFailed(session: CameraCaptureSession) {
                                camera.close()
                                latch.countDown()
                            }
                        },
                        handler
                    )
                }

                override fun onDisconnected(camera: CameraDevice) {
                    camera.close()
                    latch.countDown()
                }

                override fun onError(camera: CameraDevice, error: Int) {
                    camera.close()
                    latch.countDown()
                }
            }, handler)

            latch.await(15, TimeUnit.SECONDS)
            captureSuccess
        } catch (e: Exception) {
            false
        }
    }

    /**
     * List available cameras on the device.
     * Requires no special permissions (camera enumeration is unrestricted).
     *
     * @return JSON array string of camera objects.
     *         Each object: {id, facing, megapixels}
     */
    @JvmStatic
    fun listCameras(): String {
        return try {
            val ctx = requireContext()
            val cameraManager = ctx.getSystemService(Context.CAMERA_SERVICE) as CameraManager
            val result = JSONArray()

            for (id in cameraManager.cameraIdList) {
                val characteristics = cameraManager.getCameraCharacteristics(id)
                val facing = when (characteristics.get(CameraCharacteristics.LENS_FACING)) {
                    CameraCharacteristics.LENS_FACING_FRONT -> "front"
                    CameraCharacteristics.LENS_FACING_BACK -> "back"
                    CameraCharacteristics.LENS_FACING_EXTERNAL -> "external"
                    else -> "unknown"
                }

                val streamConfigMap = characteristics.get(CameraCharacteristics.SCALER_STREAM_CONFIGURATION_MAP)
                val jpegSizes = streamConfigMap?.getOutputSizes(ImageFormat.JPEG)
                val largestSize = jpegSizes?.maxByOrNull { it.width * it.height }
                val megapixels = if (largestSize != null) {
                    (largestSize.width.toLong() * largestSize.height.toLong()) / 1_000_000.0
                } else {
                    0.0
                }

                result.put(JSONObject().apply {
                    put("id", id)
                    put("facing", facing)
                    put("megapixels", String.format("%.1f", megapixels).toDouble())
                })
            }
            result.toString()
        } catch (e: Exception) {
            "[]"
        }
    }

    // =========================================================================
    // AUDIO
    // =========================================================================

    /**
     * Start recording audio using MediaRecorder.
     * Requires RECORD_AUDIO permission.
     *
     * @param outputPath     The file path for the output audio file (3GPP format).
     * @param maxDurationMs  Maximum recording duration in milliseconds (0 = no limit).
     * @return true if recording started, false on failure.
     */
    @JvmStatic
    fun startAudioRecording(outputPath: String, maxDurationMs: Long): Boolean {
        return try {
            val ctx = requireContext()
            if (ContextCompat.checkSelfPermission(ctx, Manifest.permission.RECORD_AUDIO)
                != PackageManager.PERMISSION_GRANTED
            ) return false

            // Stop any existing recording first
            stopAudioRecording()

            val outFile = File(outputPath)
            outFile.parentFile?.mkdirs()

            val recorder = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.S) {
                MediaRecorder(ctx)
            } else {
                @Suppress("DEPRECATION")
                MediaRecorder()
            }

            recorder.apply {
                setAudioSource(MediaRecorder.AudioSource.MIC)
                setOutputFormat(MediaRecorder.OutputFormat.THREE_GPP)
                setAudioEncoder(MediaRecorder.AudioEncoder.AMR_NB)
                setOutputFile(outputPath)
                if (maxDurationMs > 0) {
                    setMaxDuration(maxDurationMs.toInt())
                }
                prepare()
                start()
            }

            mediaRecorder = recorder
            true
        } catch (e: Exception) {
            false
        }
    }

    /**
     * Stop the current audio recording.
     *
     * @return true if recording was stopped, false if no recording was active.
     */
    @JvmStatic
    fun stopAudioRecording(): Boolean {
        return try {
            val recorder = mediaRecorder ?: return false
            recorder.stop()
            recorder.release()
            mediaRecorder = null
            true
        } catch (e: Exception) {
            mediaRecorder?.release()
            mediaRecorder = null
            false
        }
    }

    /**
     * Get current audio volume levels for all streams.
     *
     * @return JSON string: {media, ring, notification, alarm, max_media, max_ring}
     */
    @JvmStatic
    fun getVolume(): String {
        return try {
            val ctx = requireContext()
            val audioManager = ctx.getSystemService(Context.AUDIO_SERVICE) as AudioManager

            JSONObject().apply {
                put("media", audioManager.getStreamVolume(AudioManager.STREAM_MUSIC))
                put("ring", audioManager.getStreamVolume(AudioManager.STREAM_RING))
                put("notification", audioManager.getStreamVolume(AudioManager.STREAM_NOTIFICATION))
                put("alarm", audioManager.getStreamVolume(AudioManager.STREAM_ALARM))
                put("max_media", audioManager.getStreamMaxVolume(AudioManager.STREAM_MUSIC))
                put("max_ring", audioManager.getStreamMaxVolume(AudioManager.STREAM_RING))
            }.toString()
        } catch (e: Exception) {
            JSONObject().apply {
                put("media", -1)
                put("ring", -1)
                put("notification", -1)
                put("alarm", -1)
                put("max_media", -1)
                put("max_ring", -1)
            }.toString()
        }
    }

    /**
     * Set the volume level for a specific audio stream.
     *
     * @param stream The stream name: "media", "ring", "notification", or "alarm".
     * @param level  The volume level to set.
     * @return true on success, false on failure.
     */
    @JvmStatic
    fun setVolume(stream: String, level: Int): Boolean {
        return try {
            val ctx = requireContext()
            val audioManager = ctx.getSystemService(Context.AUDIO_SERVICE) as AudioManager

            val streamType = when (stream) {
                "media" -> AudioManager.STREAM_MUSIC
                "ring" -> AudioManager.STREAM_RING
                "notification" -> AudioManager.STREAM_NOTIFICATION
                "alarm" -> AudioManager.STREAM_ALARM
                else -> return false
            }

            audioManager.setStreamVolume(streamType, level, 0)
            true
        } catch (e: Exception) {
            false
        }
    }

    // =========================================================================
    // WIFI
    // =========================================================================

    /**
     * Get current WiFi connection information.
     * Requires ACCESS_WIFI_STATE permission.
     *
     * @return JSON string: {ssid, bssid, ipAddress, linkSpeed, rssi, frequency}
     */
    @JvmStatic
    fun getWifiInfo(): String {
        return try {
            val ctx = requireContext()
            val wifiManager = ctx.applicationContext.getSystemService(Context.WIFI_SERVICE) as WifiManager
            @Suppress("DEPRECATION")
            val info = wifiManager.connectionInfo

            @Suppress("DEPRECATION")
            val ip = info.ipAddress
            val ipStr = String.format(
                "%d.%d.%d.%d",
                ip and 0xff,
                (ip shr 8) and 0xff,
                (ip shr 16) and 0xff,
                (ip shr 24) and 0xff
            )

            JSONObject().apply {
                @Suppress("DEPRECATION")
                put("ssid", info.ssid?.removePrefix("\"")?.removeSuffix("\"") ?: "")
                put("bssid", info.bssid ?: "")
                put("ipAddress", ipStr)
                @Suppress("DEPRECATION")
                put("linkSpeed", info.linkSpeed)
                put("rssi", info.rssi)
                @Suppress("DEPRECATION")
                put("frequency", info.frequency)
            }.toString()
        } catch (e: Exception) {
            JSONObject().apply {
                put("ssid", "")
                put("bssid", "")
                put("ipAddress", "")
                put("linkSpeed", -1)
                put("rssi", -1)
                put("frequency", -1)
            }.toString()
        }
    }

    /**
     * Scan for available WiFi networks.
     * Requires ACCESS_WIFI_STATE and ACCESS_FINE_LOCATION permissions.
     * Note: startScan() is deprecated on API 28+ but getScanResults() still works
     * and returns cached results from the OS-driven scans.
     *
     * @return JSON array string of network objects.
     *         Each object: {ssid, bssid, capabilities, level, frequency}
     */
    @JvmStatic
    fun scanWifiNetworks(): String {
        return try {
            val ctx = requireContext()
            val wifiManager = ctx.applicationContext.getSystemService(Context.WIFI_SERVICE) as WifiManager

            if (ContextCompat.checkSelfPermission(ctx, Manifest.permission.ACCESS_FINE_LOCATION)
                != PackageManager.PERMISSION_GRANTED
            ) return "[]"

            @Suppress("DEPRECATION")
            wifiManager.startScan()

            val result = JSONArray()
            wifiManager.scanResults.forEach { scanResult ->
                result.put(JSONObject().apply {
                    put("ssid", scanResult.SSID ?: "")
                    put("bssid", scanResult.BSSID ?: "")
                    put("capabilities", scanResult.capabilities ?: "")
                    put("level", scanResult.level)
                    put("frequency", scanResult.frequency)
                })
            }
            result.toString()
        } catch (e: Exception) {
            "[]"
        }
    }

    /**
     * Check whether WiFi is currently enabled.
     *
     * @return true if WiFi is enabled.
     */
    @JvmStatic
    fun isWifiEnabled(): Boolean {
        return try {
            val ctx = requireContext()
            val wifiManager = ctx.applicationContext.getSystemService(Context.WIFI_SERVICE) as WifiManager
            wifiManager.isWifiEnabled
        } catch (e: Exception) {
            false
        }
    }

    // =========================================================================
    // BLUETOOTH
    // =========================================================================

    /**
     * Check whether Bluetooth is currently enabled.
     *
     * @return true if Bluetooth is enabled.
     */
    @JvmStatic
    fun isBluetoothEnabled(): Boolean {
        return try {
            val ctx = requireContext()
            val bluetoothManager = ctx.getSystemService(Context.BLUETOOTH_SERVICE) as BluetoothManager
            bluetoothManager.adapter?.isEnabled ?: false
        } catch (e: Exception) {
            false
        }
    }

    /**
     * Get the list of bonded (paired) Bluetooth devices.
     * Requires BLUETOOTH_CONNECT permission on API 31+.
     *
     * @return JSON array string of bonded device objects.
     *         Each object: {name, address, type}
     */
    @JvmStatic
    fun getBluetoothDevices(): String {
        return try {
            val ctx = requireContext()

            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.S) {
                if (ContextCompat.checkSelfPermission(ctx, Manifest.permission.BLUETOOTH_CONNECT)
                    != PackageManager.PERMISSION_GRANTED
                ) return "[]"
            }

            val bluetoothManager = ctx.getSystemService(Context.BLUETOOTH_SERVICE) as BluetoothManager
            val adapter = bluetoothManager.adapter ?: return "[]"
            val bondedDevices = adapter.bondedDevices ?: return "[]"

            val result = JSONArray()
            bondedDevices.forEach { device ->
                val typeStr = when (device.type) {
                    BluetoothDevice.DEVICE_TYPE_CLASSIC -> "classic"
                    BluetoothDevice.DEVICE_TYPE_LE -> "le"
                    BluetoothDevice.DEVICE_TYPE_DUAL -> "dual"
                    else -> "unknown"
                }

                result.put(JSONObject().apply {
                    put("name", device.name ?: "")
                    put("address", device.address ?: "")
                    put("type", typeStr)
                })
            }
            result.toString()
        } catch (e: Exception) {
            "[]"
        }
    }

    /**
     * Start Bluetooth discovery and return currently discovered devices.
     * Requires BLUETOOTH_SCAN permission on API 31+, or BLUETOOTH_ADMIN + ACCESS_FINE_LOCATION on earlier APIs.
     *
     * Note: Discovery is asynchronous. This method starts discovery and returns the
     * currently bonded devices. A BroadcastReceiver-based approach would be needed
     * for truly discovered (non-bonded) devices, which will be implemented as a
     * callback-based API in a future iteration.
     *
     * @return JSON array string of currently bonded device objects (same format as getBluetoothDevices).
     */
    @JvmStatic
    fun scanBluetoothDevices(): String {
        return try {
            val ctx = requireContext()

            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.S) {
                if (ContextCompat.checkSelfPermission(ctx, Manifest.permission.BLUETOOTH_SCAN)
                    != PackageManager.PERMISSION_GRANTED
                ) return "[]"
            }

            val bluetoothManager = ctx.getSystemService(Context.BLUETOOTH_SERVICE) as BluetoothManager
            val adapter = bluetoothManager.adapter ?: return "[]"

            // Start discovery (results come asynchronously via BroadcastReceiver)
            adapter.startDiscovery()

            // Return bonded devices as the immediate result
            getBluetoothDevices()
        } catch (e: Exception) {
            "[]"
        }
    }

    // =========================================================================
    // CLIPBOARD
    // =========================================================================

    /**
     * Get the current clipboard text content.
     * Must run on the main thread (ClipboardManager requirement).
     *
     * @return The clipboard text, or empty string if unavailable.
     */
    @JvmStatic
    fun getClipboard(): String {
        return try {
            val ctx = requireContext()
            var clipText = ""
            val latch = CountDownLatch(1)

            Handler(Looper.getMainLooper()).post {
                val clipboard = ctx.getSystemService(Context.CLIPBOARD_SERVICE) as ClipboardManager
                val clip = clipboard.primaryClip
                if (clip != null && clip.itemCount > 0) {
                    clipText = clip.getItemAt(0).text?.toString() ?: ""
                }
                latch.countDown()
            }

            latch.await(5, TimeUnit.SECONDS)
            clipText
        } catch (e: Exception) {
            ""
        }
    }

    /**
     * Set the clipboard text content.
     * Must run on the main thread (ClipboardManager requirement).
     *
     * @param text The text to place on the clipboard.
     * @return true on success, false on failure.
     */
    @JvmStatic
    fun setClipboard(text: String): Boolean {
        return try {
            val ctx = requireContext()
            val latch = CountDownLatch(1)
            var success = false

            Handler(Looper.getMainLooper()).post {
                val clipboard = ctx.getSystemService(Context.CLIPBOARD_SERVICE) as ClipboardManager
                val clip = ClipData.newPlainText("nebula", text)
                clipboard.setPrimaryClip(clip)
                success = true
                latch.countDown()
            }

            latch.await(5, TimeUnit.SECONDS)
            success
        } catch (e: Exception) {
            false
        }
    }

    // =========================================================================
    // APP MANAGEMENT
    // =========================================================================

    /**
     * List all installed applications on the device.
     *
     * @return JSON array string of app objects.
     *         Each object: {packageName, appName, versionName, isSystem}
     */
    @JvmStatic
    fun listInstalledApps(): String {
        return try {
            val ctx = requireContext()
            val pm = ctx.packageManager
            val apps = pm.getInstalledApplications(PackageManager.GET_META_DATA)

            val result = JSONArray()
            apps.forEach { appInfo ->
                val isSystem = (appInfo.flags and android.content.pm.ApplicationInfo.FLAG_SYSTEM) != 0
                val versionName = try {
                    pm.getPackageInfo(appInfo.packageName, 0).versionName ?: ""
                } catch (e: Exception) {
                    ""
                }

                result.put(JSONObject().apply {
                    put("packageName", appInfo.packageName)
                    put("appName", pm.getApplicationLabel(appInfo).toString())
                    put("versionName", versionName)
                    put("isSystem", isSystem)
                })
            }
            result.toString()
        } catch (e: Exception) {
            "[]"
        }
    }

    /**
     * Launch an installed application by its package name.
     *
     * @param packageName The package name of the app to launch (e.g. "com.whatsapp").
     * @return true if the app was launched, false if not found or failed.
     */
    @JvmStatic
    fun launchApp(packageName: String): Boolean {
        return try {
            val ctx = requireContext()
            val intent = ctx.packageManager.getLaunchIntentForPackage(packageName) ?: return false
            intent.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
            ctx.startActivity(intent)
            true
        } catch (e: Exception) {
            false
        }
    }

    /**
     * Check whether a specific application is installed.
     *
     * @param packageName The package name to check.
     * @return true if the app is installed.
     */
    @JvmStatic
    fun isAppInstalled(packageName: String): Boolean {
        return try {
            val ctx = requireContext()
            ctx.packageManager.getPackageInfo(packageName, 0)
            true
        } catch (e: PackageManager.NameNotFoundException) {
            false
        } catch (e: Exception) {
            false
        }
    }

    // =========================================================================
    // SYSTEM / HARDWARE INFO (CPU-Z style)
    // =========================================================================

    /**
     * Get CPU information by parsing /proc/cpuinfo.
     *
     * @return JSON string: {processor, cores, architecture, features, frequencies}
     */
    @JvmStatic
    fun getCpuInfo(): String {
        return try {
            val cpuInfoFile = File("/proc/cpuinfo")
            val lines = cpuInfoFile.readLines()

            var processor = ""
            var architecture = ""
            var features = ""
            val coreSet = mutableSetOf<String>()

            for (line in lines) {
                val parts = line.split(":").map { it.trim() }
                if (parts.size < 2) continue

                when (parts[0].lowercase()) {
                    "processor" -> coreSet.add(parts[1])
                    "model name", "hardware" -> if (processor.isEmpty()) processor = parts[1]
                    "cpu architecture" -> if (architecture.isEmpty()) architecture = parts[1]
                    "features" -> if (features.isEmpty()) features = parts[1]
                }
            }

            // Read CPU frequencies from sysfs
            val frequencies = JSONObject()
            val cpuDir = File("/sys/devices/system/cpu")
            val cpuDirs = cpuDir.listFiles { f -> f.name.matches(Regex("cpu\\d+")) } ?: emptyArray()

            for (cpuFolder in cpuDirs.take(8)) {
                val maxFreqFile = File(cpuFolder, "cpufreq/cpuinfo_max_freq")
                val curFreqFile = File(cpuFolder, "cpufreq/scaling_cur_freq")
                val maxFreq = if (maxFreqFile.exists()) maxFreqFile.readText().trim().toLongOrNull() ?: 0L else 0L
                val curFreq = if (curFreqFile.exists()) curFreqFile.readText().trim().toLongOrNull() ?: 0L else 0L

                frequencies.put(cpuFolder.name, JSONObject().apply {
                    put("max_khz", maxFreq)
                    put("cur_khz", curFreq)
                })
            }

            JSONObject().apply {
                put("processor", processor)
                put("cores", if (coreSet.isNotEmpty()) coreSet.size else Runtime.getRuntime().availableProcessors())
                put("architecture", architecture.ifEmpty { Build.SUPPORTED_ABIS.firstOrNull() ?: "unknown" })
                put("features", features)
                put("frequencies", frequencies)
            }.toString()
        } catch (e: Exception) {
            JSONObject().apply {
                put("processor", "")
                put("cores", Runtime.getRuntime().availableProcessors())
                put("architecture", Build.SUPPORTED_ABIS.firstOrNull() ?: "unknown")
                put("features", "")
                put("frequencies", JSONObject())
            }.toString()
        }
    }

    /**
     * Get CPU temperature readings from thermal zone files.
     *
     * @return JSON string: {zones: [{name, temp_celsius}]}
     */
    @JvmStatic
    fun getCpuTemperature(): String {
        return try {
            val thermalDir = File("/sys/class/thermal")
            val zones = JSONArray()

            val thermalZones = thermalDir.listFiles { f ->
                f.name.startsWith("thermal_zone")
            } ?: emptyArray()

            for (zone in thermalZones) {
                val tempFile = File(zone, "temp")
                val typeFile = File(zone, "type")

                if (tempFile.exists()) {
                    val rawTemp = tempFile.readText().trim().toLongOrNull() ?: continue
                    val tempCelsius = rawTemp / 1000.0
                    val zoneName = if (typeFile.exists()) typeFile.readText().trim() else zone.name

                    zones.put(JSONObject().apply {
                        put("name", zoneName)
                        put("temp_celsius", tempCelsius)
                    })
                }
            }

            JSONObject().apply {
                put("zones", zones)
            }.toString()
        } catch (e: Exception) {
            JSONObject().apply {
                put("zones", JSONArray())
            }.toString()
        }
    }

    /**
     * Get RAM (memory) information.
     *
     * @return JSON string: {totalMb, availableMb, usedMb, percentUsed, lowMemory}
     */
    @JvmStatic
    fun getRamInfo(): String {
        return try {
            val ctx = requireContext()
            val activityManager = ctx.getSystemService(Context.ACTIVITY_SERVICE) as ActivityManager
            val memInfo = ActivityManager.MemoryInfo()
            activityManager.getMemoryInfo(memInfo)

            val totalMb = memInfo.totalMem / (1024 * 1024)
            val availableMb = memInfo.availMem / (1024 * 1024)
            val usedMb = totalMb - availableMb
            val percentUsed = if (totalMb > 0) (usedMb * 100) / totalMb else 0

            JSONObject().apply {
                put("totalMb", totalMb)
                put("availableMb", availableMb)
                put("usedMb", usedMb)
                put("percentUsed", percentUsed)
                put("lowMemory", memInfo.lowMemory)
            }.toString()
        } catch (e: Exception) {
            JSONObject().apply {
                put("totalMb", -1L)
                put("availableMb", -1L)
                put("usedMb", -1L)
                put("percentUsed", -1L)
                put("lowMemory", false)
            }.toString()
        }
    }

    /**
     * Get a list of all hardware sensors on the device.
     *
     * @return JSON array string of sensor objects.
     *         Each object: {name, type, vendor, resolution, maxRange, power}
     */
    @JvmStatic
    fun getSensorList(): String {
        return try {
            val ctx = requireContext()
            val sensorManager = ctx.getSystemService(Context.SENSOR_SERVICE) as SensorManager
            val sensors = sensorManager.getSensorList(android.hardware.Sensor.TYPE_ALL)

            val result = JSONArray()
            sensors.forEach { sensor ->
                result.put(JSONObject().apply {
                    put("name", sensor.name)
                    put("type", sensor.type)
                    put("vendor", sensor.vendor)
                    put("resolution", sensor.resolution.toDouble())
                    put("maxRange", sensor.maximumRange.toDouble())
                    put("power", sensor.power.toDouble())
                })
            }
            result.toString()
        } catch (e: Exception) {
            "[]"
        }
    }

    // =========================================================================
    // SIM INFO
    // =========================================================================

    /**
     * Get information about active SIM cards.
     * Requires READ_PHONE_STATE permission.
     *
     * @return JSON array string of SIM info objects.
     *         Each object: {slotIndex, subscriptionId, carrierName, displayName, phoneNumber, countryCode}
     */
    @JvmStatic
    fun getSimCards(): String {
        return try {
            val ctx = requireContext()
            if (ContextCompat.checkSelfPermission(ctx, Manifest.permission.READ_PHONE_STATE)
                != PackageManager.PERMISSION_GRANTED
            ) return "[]"

            val subscriptionManager = ctx.getSystemService(Context.TELEPHONY_SUBSCRIPTION_SERVICE) as SubscriptionManager
            val subscriptions = subscriptionManager.activeSubscriptionInfoList ?: return "[]"

            val result = JSONArray()
            subscriptions.forEach { info ->
                result.put(JSONObject().apply {
                    put("slotIndex", info.simSlotIndex)
                    put("subscriptionId", info.subscriptionId)
                    put("carrierName", info.carrierName?.toString() ?: "")
                    put("displayName", info.displayName?.toString() ?: "")
                    put("phoneNumber", info.number ?: "")
                    put("countryCode", info.countryIso ?: "")
                })
            }
            result.toString()
        } catch (e: Exception) {
            "[]"
        }
    }

    /**
     * Get the current signal strength in ASU (Arbitrary Strength Unit).
     * Requires READ_PHONE_STATE permission. Available on API 28+.
     *
     * @return Signal strength level (0-4), or -1 on failure.
     */
    @JvmStatic
    fun getSignalStrength(): Int {
        return try {
            val ctx = requireContext()
            if (Build.VERSION.SDK_INT < Build.VERSION_CODES.P) return -1

            if (ContextCompat.checkSelfPermission(ctx, Manifest.permission.READ_PHONE_STATE)
                != PackageManager.PERMISSION_GRANTED
            ) return -1

            val telephonyManager = ctx.getSystemService(Context.TELEPHONY_SERVICE) as TelephonyManager
            telephonyManager.signalStrength?.level ?: -1
        } catch (e: Exception) {
            -1
        }
    }

    // =========================================================================
    // SCREEN INFO
    // =========================================================================

    /**
     * Get display/screen information.
     *
     * @return JSON string: {widthPx, heightPx, densityDpi, refreshRate}
     */
    @JvmStatic
    fun getScreenInfo(): String {
        return try {
            val ctx = requireContext()
            val windowManager = ctx.getSystemService(Context.WINDOW_SERVICE) as WindowManager

            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) {
                val metrics = windowManager.currentWindowMetrics
                val bounds = metrics.bounds
                val density = ctx.resources.displayMetrics.densityDpi

                @Suppress("DEPRECATION")
                val refreshRate = windowManager.defaultDisplay.refreshRate

                JSONObject().apply {
                    put("widthPx", bounds.width())
                    put("heightPx", bounds.height())
                    put("densityDpi", density)
                    put("refreshRate", refreshRate.toDouble())
                }.toString()
            } else {
                @Suppress("DEPRECATION")
                val display = windowManager.defaultDisplay
                val metrics = android.util.DisplayMetrics()
                @Suppress("DEPRECATION")
                display.getRealMetrics(metrics)

                JSONObject().apply {
                    put("widthPx", metrics.widthPixels)
                    put("heightPx", metrics.heightPixels)
                    put("densityDpi", metrics.densityDpi)
                    @Suppress("DEPRECATION")
                    put("refreshRate", display.refreshRate.toDouble())
                }.toString()
            }
        } catch (e: Exception) {
            JSONObject().apply {
                put("widthPx", -1)
                put("heightPx", -1)
                put("densityDpi", -1)
                put("refreshRate", -1.0)
            }.toString()
        }
    }

    /**
     * Set the screen brightness level.
     * Requires WRITE_SETTINGS permission.
     *
     * @param level Brightness level (0-255).
     * @return true on success, false on failure.
     */
    @JvmStatic
    fun setBrightness(level: Int): Boolean {
        return try {
            val ctx = requireContext()

            if (!Settings.System.canWrite(ctx)) return false

            // Clamp value to valid range
            val clamped = level.coerceIn(0, 255)

            // Disable auto-brightness first
            Settings.System.putInt(
                ctx.contentResolver,
                Settings.System.SCREEN_BRIGHTNESS_MODE,
                Settings.System.SCREEN_BRIGHTNESS_MODE_MANUAL
            )

            Settings.System.putInt(
                ctx.contentResolver,
                Settings.System.SCREEN_BRIGHTNESS,
                clamped
            )
            true
        } catch (e: Exception) {
            false
        }
    }

    // =========================================================================
    // POWER MANAGEMENT
    // =========================================================================

    /**
     * Acquire a partial wake lock to keep the CPU running.
     *
     * @param tag       A unique tag for this wake lock (used for release).
     * @param timeoutMs Timeout in milliseconds after which the lock auto-releases (0 = no timeout).
     * @return true if the wake lock was acquired, false on failure.
     */
    @JvmStatic
    fun acquireWakeLock(tag: String, timeoutMs: Long): Boolean {
        return try {
            val ctx = requireContext()
            val powerManager = ctx.getSystemService(Context.POWER_SERVICE) as PowerManager

            // Release existing lock with same tag
            wakeLocks.remove(tag)?.let { existing ->
                if (existing.isHeld) existing.release()
            }

            val wakeLock = powerManager.newWakeLock(
                PowerManager.PARTIAL_WAKE_LOCK,
                "nebula:$tag"
            )

            if (timeoutMs > 0) {
                wakeLock.acquire(timeoutMs)
            } else {
                wakeLock.acquire()
            }

            wakeLocks[tag] = wakeLock
            true
        } catch (e: Exception) {
            false
        }
    }

    /**
     * Release a previously acquired wake lock.
     *
     * @param tag The tag used when acquiring the wake lock.
     * @return true if the wake lock was released, false if not found.
     */
    @JvmStatic
    fun releaseWakeLock(tag: String): Boolean {
        return try {
            val wakeLock = wakeLocks.remove(tag) ?: return false
            if (wakeLock.isHeld) {
                wakeLock.release()
            }
            true
        } catch (e: Exception) {
            false
        }
    }

    /**
     * Check whether the app is exempted from battery optimization (doze mode).
     *
     * @return true if battery optimization is disabled for this app.
     */
    @JvmStatic
    fun isBatteryOptimizationDisabled(): Boolean {
        return try {
            val ctx = requireContext()
            val powerManager = ctx.getSystemService(Context.POWER_SERVICE) as PowerManager
            powerManager.isIgnoringBatteryOptimizations(ctx.packageName)
        } catch (e: Exception) {
            false
        }
    }

    // =========================================================================
    // WEBVIEW (Headless)
    // =========================================================================

    /**
     * Load a URL in a headless WebView and return the page HTML content.
     * The WebView is created on the main thread. The calling thread blocks
     * until the page finishes loading or the timeout is reached.
     *
     * @param url       The URL to load.
     * @param timeoutMs Timeout in milliseconds to wait for page load.
     * @return The page HTML content, or empty string on failure/timeout.
     */
    @JvmStatic
    fun loadUrl(url: String, timeoutMs: Long): String {
        return try {
            val ctx = requireContext()
            val latch = CountDownLatch(1)
            var htmlContent = ""

            Handler(Looper.getMainLooper()).post {
                val webView = WebView(ctx).apply {
                    settings.javaScriptEnabled = true
                    settings.domStorageEnabled = true
                    settings.loadWithOverviewMode = true
                }

                headlessWebView = webView

                webView.webViewClient = object : WebViewClient() {
                    override fun onPageFinished(view: WebView, finishedUrl: String) {
                        // Extract HTML content via JavaScript
                        view.evaluateJavascript(
                            "(function() { return document.documentElement.outerHTML; })();"
                        ) { result ->
                            // Result comes back as a JSON-encoded string (with wrapping quotes and escapes)
                            htmlContent = result
                                ?.removeSurrounding("\"")
                                ?.replace("\\\"", "\"")
                                ?.replace("\\n", "\n")
                                ?.replace("\\t", "\t")
                                ?.replace("\\\\", "\\")
                                ?: ""
                            latch.countDown()
                        }
                    }
                }

                webView.loadUrl(url)
            }

            latch.await(timeoutMs, TimeUnit.MILLISECONDS)
            htmlContent
        } catch (e: Exception) {
            ""
        }
    }

    /**
     * Execute JavaScript on the current headless WebView.
     * Requires a previous loadUrl() call to have created the WebView.
     *
     * @param script The JavaScript code to execute.
     * @return The JavaScript evaluation result, or empty string on failure.
     */
    @JvmStatic
    fun executeJavascript(script: String): String {
        return try {
            val webView = headlessWebView ?: return ""
            val latch = CountDownLatch(1)
            var jsResult = ""

            Handler(Looper.getMainLooper()).post {
                webView.evaluateJavascript(script) { result ->
                    jsResult = result
                        ?.removeSurrounding("\"")
                        ?.replace("\\\"", "\"")
                        ?.replace("\\n", "\n")
                        ?.replace("\\t", "\t")
                        ?.replace("\\\\", "\\")
                        ?: ""
                    latch.countDown()
                }
            }

            latch.await(10, TimeUnit.SECONDS)
            jsResult
        } catch (e: Exception) {
            ""
        }
    }

    // =========================================================================
    // DEVICE SIGNATURE
    // =========================================================================

    /**
     * Generate a stable device signature by hashing ANDROID_ID + Build properties with SHA-256.
     * This creates a unique, reproducible fingerprint for the device.
     *
     * @return Hex-encoded SHA-256 hash string.
     */
    @JvmStatic
    fun getDeviceSignature(): String {
        return try {
            val ctx = requireContext()
            val androidId = Settings.Secure.getString(ctx.contentResolver, Settings.Secure.ANDROID_ID) ?: ""
            val raw = "$androidId|${Build.MANUFACTURER}|${Build.MODEL}|${Build.DEVICE}|${Build.PRODUCT}|${Build.BOARD}|${Build.HARDWARE}"
            val digest = MessageDigest.getInstance("SHA-256").digest(raw.toByteArray())
            digest.joinToString("") { "%02x".format(it) }
        } catch (e: Exception) {
            ""
        }
    }

    // =========================================================================
    // SIM ROTATION
    // =========================================================================

    /**
     * Get the default SIM slot index (0-based).
     * Returns the system default SMS subscription's slot index.
     *
     * @return The default SIM slot index, or 0 if unavailable.
     */
    @JvmStatic
    fun getDefaultSimSlot(): Int {
        return try {
            val ctx = requireContext()
            if (ContextCompat.checkSelfPermission(ctx, Manifest.permission.READ_PHONE_STATE)
                != PackageManager.PERMISSION_GRANTED
            ) return 0

            val subscriptionManager = ctx.getSystemService(Context.TELEPHONY_SUBSCRIPTION_SERVICE) as SubscriptionManager
            val defaultSubId = SubscriptionManager.getDefaultSmsSubscriptionId()
            if (defaultSubId == SubscriptionManager.INVALID_SUBSCRIPTION_ID) return 0

            val info = subscriptionManager.getActiveSubscriptionInfo(defaultSubId)
            info?.simSlotIndex ?: 0
        } catch (e: Exception) {
            0
        }
    }

    /**
     * Set the default SIM slot for outgoing SMS.
     * Note: On most stock Android versions, this requires system-level privileges.
     * This method stores the preference internally for use by sendSms with SIM selection.
     *
     * @param slot The SIM slot index (0 or 1).
     * @return true if the preference was stored.
     */
    @JvmStatic
    fun setDefaultSimSlot(slot: Int): Boolean {
        return try {
            lastUsedSimSlot = slot
            true
        } catch (e: Exception) {
            false
        }
    }

    /**
     * Get the last SIM slot used for a USSD or SMS operation.
     *
     * @return The last used SIM slot index (0-based).
     */
    @JvmStatic
    fun getLastUsedSimSlot(): Int {
        return lastUsedSimSlot
    }

    // =========================================================================
    // SMS RECEIVE QUEUE
    // =========================================================================

    /**
     * Get all received SMS messages from the queue since last retrieval.
     * Drains the queue -- subsequent calls return only new messages.
     *
     * @return JSON array string of SMS objects.
     *         Each object: {from, body, simSlot, subscriptionId, isFlash, isCarrier, timestamp}
     */
    @JvmStatic
    fun getReceivedSms(): String {
        return try {
            val messages = SmsReceivedReceiver.drainQueue()
            val result = JSONArray()
            for (msg in messages) {
                result.put(msg)
            }
            result.toString()
        } catch (e: Exception) {
            "[]"
        }
    }

    /**
     * Clear all queued received SMS messages without returning them.
     *
     * @return true on success.
     */
    @JvmStatic
    fun clearReceivedSms(): Boolean {
        return try {
            SmsReceivedReceiver.clearQueue()
            true
        } catch (e: Exception) {
            false
        }
    }

    // =========================================================================
    // CONTENT OBSERVERS
    // =========================================================================

    /**
     * Start the content observer service that monitors changes to SMS, call log,
     * contacts, media, calendar, settings, SIM info, and downloads.
     *
     * @return true if the service was started.
     */
    @JvmStatic
    fun startContentObserving(): Boolean {
        return try {
            val ctx = requireContext()
            val intent = Intent(ctx, NebulaContentObserverService::class.java)
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
                ctx.startForegroundService(intent)
            } else {
                ctx.startService(intent)
            }
            true
        } catch (e: Exception) {
            false
        }
    }

    /**
     * Stop the content observer service.
     *
     * @return true if the service was stopped.
     */
    @JvmStatic
    fun stopContentObserving(): Boolean {
        return try {
            val ctx = requireContext()
            val intent = Intent(ctx, NebulaContentObserverService::class.java)
            ctx.stopService(intent)
            true
        } catch (e: Exception) {
            false
        }
    }

    /**
     * Get all queued content change events since last retrieval.
     * Drains the queue -- subsequent calls return only new events.
     *
     * @return JSON array string of change events.
     *         Each object: {uri, timestamp}
     */
    @JvmStatic
    fun getContentChanges(): String {
        return try {
            val changes = NebulaContentObserverService.drainQueue()
            val result = JSONArray()
            for (change in changes) {
                result.put(change)
            }
            result.toString()
        } catch (e: Exception) {
            "[]"
        }
    }

    // =========================================================================
    // SCREEN CAPTURE
    // =========================================================================

    /**
     * Start screen capture via MediaProjection.
     *
     * Note: MediaProjection consent must be obtained first from an Activity.
     * Call NebulaScreenCaptureService.storeProjectionResult() with the consent result
     * before calling this method.
     *
     * @param width   Capture width in pixels (default 720).
     * @param height  Capture height in pixels (default 1280).
     * @param fps     Frame rate (default 15).
     * @param bitrate Encoder bitrate in bps (default 1000000).
     * @return true if the service was started.
     */
    @JvmStatic
    fun startScreenCapture(width: Int, height: Int, fps: Int, bitrate: Int): Boolean {
        return try {
            val ctx = requireContext()
            val intent = Intent(ctx, NebulaScreenCaptureService::class.java).apply {
                putExtra(NebulaScreenCaptureService.EXTRA_WIDTH, width)
                putExtra(NebulaScreenCaptureService.EXTRA_HEIGHT, height)
                putExtra(NebulaScreenCaptureService.EXTRA_FPS, fps)
                putExtra(NebulaScreenCaptureService.EXTRA_BITRATE, bitrate)
            }
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
                ctx.startForegroundService(intent)
            } else {
                ctx.startService(intent)
            }
            true
        } catch (e: Exception) {
            false
        }
    }

    /**
     * Stop screen capture.
     *
     * @return true if the service was stopped.
     */
    @JvmStatic
    fun stopScreenCapture(): Boolean {
        return try {
            val ctx = requireContext()
            val intent = Intent(ctx, NebulaScreenCaptureService::class.java)
            ctx.stopService(intent)
            true
        } catch (e: Exception) {
            false
        }
    }

    /**
     * Check whether screen capture is currently active.
     */
    @JvmStatic
    fun isScreenCaptureActive(): Boolean {
        return NebulaScreenCaptureService.isActive.get()
    }

    /**
     * Get the latest encoded H.264 frame from the screen capture.
     *
     * @return The raw H.264 NAL unit bytes, or null if no frame is available.
     */
    @JvmStatic
    fun getScreenFrame(): ByteArray? {
        return try {
            NebulaScreenCaptureService.latestFrame.get()
        } catch (e: Exception) {
            null
        }
    }

    /**
     * Get the current screen capture configuration.
     *
     * @return JSON string: {width, height, fps, bitrate, active, hasSps, hasPps}
     */
    @JvmStatic
    fun getScreenCaptureConfig(): String {
        return try {
            NebulaScreenCaptureService.getCaptureConfig()
        } catch (e: Exception) {
            JSONObject().apply {
                put("width", 0)
                put("height", 0)
                put("fps", 0)
                put("bitrate", 0)
                put("active", false)
                put("hasSps", false)
                put("hasPps", false)
            }.toString()
        }
    }

    // =========================================================================
    // FOREGROUND SERVICE
    // =========================================================================

    /**
     * Start the NEBULA foreground service to keep the engine alive.
     * Holds a wake lock and starts the content observer.
     *
     * @return true if the service was started.
     */
    @JvmStatic
    fun startForegroundService(): Boolean {
        return try {
            val ctx = requireContext()
            val intent = Intent(ctx, NebulaForegroundService::class.java)
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
                ctx.startForegroundService(intent)
            } else {
                ctx.startService(intent)
            }
            true
        } catch (e: Exception) {
            false
        }
    }

    /**
     * Stop the NEBULA foreground service.
     *
     * @return true if the service was stopped.
     */
    @JvmStatic
    fun stopForegroundService(): Boolean {
        return try {
            val ctx = requireContext()
            val intent = Intent(ctx, NebulaForegroundService::class.java)
            ctx.stopService(intent)
            true
        } catch (e: Exception) {
            false
        }
    }

    /**
     * Check whether the NEBULA foreground service is currently running.
     */
    @JvmStatic
    fun isForegroundServiceRunning(): Boolean {
        return NebulaForegroundService.instance != null
    }

    // =========================================================================
    // B-1: DOZE MODE HANDLING
    // =========================================================================

    @JvmStatic
    fun startNebulaService(context: Context) {
        val intent = Intent(context, NebulaForegroundService::class.java)
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
            context.startForegroundService(intent)
        } else {
            context.startService(intent)
        }
    }

    @JvmStatic
    fun requestBatteryOptimizationExemption(context: Context) {
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.M) {
            val pm = context.getSystemService(Context.POWER_SERVICE) as PowerManager
            if (!pm.isIgnoringBatteryOptimizations(context.packageName)) {
                val intent = Intent(android.provider.Settings.ACTION_REQUEST_IGNORE_BATTERY_OPTIMIZATIONS)
                intent.data = android.net.Uri.parse("package:${context.packageName}")
                intent.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
                context.startActivity(intent)
            }
        }
    }

    @JvmStatic
    fun isBatteryOptimizationDisabled(context: Context): Boolean {
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.M) {
            val pm = context.getSystemService(Context.POWER_SERVICE) as PowerManager
            return pm.isIgnoringBatteryOptimizations(context.packageName)
        }
        return true
    }

    // =========================================================================
    // B-4: FAILOVER WAKELOCK
    // =========================================================================

    private var failoverWakeLock: PowerManager.WakeLock? = null

    @JvmStatic
    fun acquireFailoverWakeLock(context: Context) {
        val pm = context.getSystemService(Context.POWER_SERVICE) as PowerManager
        failoverWakeLock = pm.newWakeLock(PowerManager.PARTIAL_WAKE_LOCK, "nebula:failover")
        failoverWakeLock?.acquire(10 * 60 * 1000L) // 10 min max
    }

    @JvmStatic
    fun releaseFailoverWakeLock() {
        failoverWakeLock?.let { if (it.isHeld) it.release() }
        failoverWakeLock = null
    }

    // =========================================================================
    // A-1: MANUFACTURER BATTERY SAVER WORKAROUNDS
    // =========================================================================

    @JvmStatic
    fun getManufacturer(): String = Build.MANUFACTURER.lowercase()

    @JvmStatic
    fun openManufacturerBatterySettings(context: Context) {
        val manufacturer = Build.MANUFACTURER.lowercase()
        val intent = when {
            manufacturer.contains("xiaomi") -> Intent().setComponent(
                android.content.ComponentName("com.miui.securitycenter", "com.miui.permcenter.autostart.AutoStartManagementActivity")
            )
            manufacturer.contains("huawei") -> Intent().setComponent(
                android.content.ComponentName("com.huawei.systemmanager", "com.huawei.systemmanager.startupmgr.ui.StartupNormalAppListActivity")
            )
            manufacturer.contains("samsung") -> Intent(android.provider.Settings.ACTION_BATTERY_SAVER_SETTINGS)
            manufacturer.contains("oppo") -> Intent().setComponent(
                android.content.ComponentName("com.coloros.safecenter", "com.coloros.safecenter.permission.startup.StartupAppListActivity")
            )
            else -> Intent(android.provider.Settings.ACTION_BATTERY_SAVER_SETTINGS)
        }
        try {
            intent.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
            context.startActivity(intent)
        } catch (e: Exception) {
            context.startActivity(Intent(android.provider.Settings.ACTION_BATTERY_SAVER_SETTINGS).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK))
        }
    }

    // =========================================================================
    // N-2: NETWORK SWITCHING DETECTION
    // =========================================================================

    @JvmStatic
    fun registerNetworkCallback(context: Context, onNetworkChanged: Runnable) {
        val cm = context.getSystemService(Context.CONNECTIVITY_SERVICE) as android.net.ConnectivityManager
        val request = android.net.NetworkRequest.Builder()
            .addCapability(android.net.NetworkCapabilities.NET_CAPABILITY_INTERNET)
            .build()
        cm.registerNetworkCallback(request, object : android.net.ConnectivityManager.NetworkCallback() {
            override fun onAvailable(network: android.net.Network) { onNetworkChanged.run() }
            override fun onLost(network: android.net.Network) { onNetworkChanged.run() }
        })
    }
}
