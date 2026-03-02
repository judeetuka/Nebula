package com.nebula.nebula_node.platform

import android.accessibilityservice.AccessibilityService
import android.accessibilityservice.AccessibilityServiceInfo
import android.content.ClipData
import android.content.ClipboardManager
import android.content.Context
import android.os.Bundle
import android.os.Handler
import android.os.Looper
import android.util.Log
import android.view.accessibility.AccessibilityEvent
import android.view.accessibility.AccessibilityNodeInfo

/**
 * AccessibilityService for interactive USSD sessions and screen content access.
 *
 * Ported from DroidRelay's UssdAccessibilityService. Handles:
 * - USSD dialog detection from 16+ OEM manufacturers
 * - Single-step USSD responses (direct from TelephonyManager)
 * - Multi-step USSD sessions (dialog detection, response extraction, reply via EditText, send button click)
 * - Flash SMS (Class 0) detection and auto-dismiss
 * - Loading state debounce (800ms)
 * - MMI error pattern matching
 *
 * Uses debounce to wait for the USSD dialog to finish loading before
 * resolving the pending result. On MediaTek/TECNO devices, the dialog
 * fires 2-3 events: first a transitional "Phone Services" screen,
 * then the actual menu content 200-300ms later.
 */
class NebulaAccessibilityService : AccessibilityService() {

    companion object {
        private const val TAG = "NebulaAccessibility"
        private const val DEBOUNCE_MS = 800L
        private const val MAX_LOADING_WAIT = 20000L
        private const val MMI_DISMISS_DELAY = 2000L

        /**
         * The currently active service instance, or null if the service is not running.
         */
        @Volatile
        var instance: NebulaAccessibilityService? = null
            private set

        @Volatile
        var sessionActive = false

        @Volatile
        var flashSmsPending = false

        /**
         * Callback invoked when a USSD response is resolved.
         * Parameters: (responseText, isTerminal)
         */
        @Volatile
        var ussdResponseCallback: ((String, Boolean) -> Unit)? = null

        /**
         * Callback invoked when a Flash SMS is detected and extracted.
         * Parameter: the flash SMS body text.
         */
        @Volatile
        var flashSmsCallback: ((String) -> Unit)? = null

        /**
         * The latest USSD response text captured, cleared after retrieval.
         */
        @Volatile
        var lastUssdResponse: String? = null
            private set

        /**
         * Whether the last USSD response was a terminal (non-interactive) response.
         */
        @Volatile
        var lastUssdTerminal: Boolean = false
            private set

        /**
         * Whether the last USSD result was an MMI error.
         */
        @Volatile
        var lastUssdError: String? = null
            private set

        /**
         * Consume and clear the last USSD response. Returns null if no response pending.
         */
        @JvmStatic
        fun consumeUssdResponse(): String? {
            val resp = lastUssdResponse
            lastUssdResponse = null
            return resp
        }

        /**
         * Consume and clear the last USSD error. Returns null if no error pending.
         */
        @JvmStatic
        fun consumeUssdError(): String? {
            val err = lastUssdError
            lastUssdError = null
            return err
        }

        // Messaging app packages -- flash SMS (Class 0) dialogs come from these
        private val MESSAGING_PACKAGES = setOf(
            "com.android.messaging",
            "com.android.mms",
            "com.google.android.apps.messaging",
            "com.samsung.android.messaging",
            "com.transsion.messaging",
        )

        // 16+ OEM USSD dialog class names
        private val USSD_DIALOG_CLASSES = setOf(
            "android.app.AlertDialog",
            "androidx.appcompat.app.AlertDialog",
            "com.android.phone.MMIDialogActivity",
            "com.android.phone.UssdAlertDialog",
            "com.transsion.hubble.phone.UssdAlertActivity",
            "com.transsion.widgetslib.dialog.PromptDialog",
            "com.mediatek.phone.UssdAlertActivity",
            "com.samsung.android.phone.UssdAlertDialog",
            "miui.app.AlertDialog",
            "miuix.appcompat.app.AlertDialog",
            "com.android.phone.MmiDialogActivity",
            "com.huawei.android.app.AlertDialog",
            "com.coloros.phone.UssdAlertActivity",
            "amigo.app.AmigoAlertDialog",
            "com.android.phone.oppo.settings.LocalAlertDialog",
            "com.zte.mifavor.widget.AlertDialog",
            "color.support.v7.app.AlertDialog",
        )

        private val BUTTON_CLASSES = setOf(
            "android.widget.Button",
            "androidx.appcompat.widget.AppCompatButton",
            "com.google.android.material.button.MaterialButton",
        )

        private val SKIP_TEXT = setOf("SEND", "CANCEL", "OK", "DISMISS")

        // Loading/transitional text that should NOT be returned as a response
        private val LOADING_PATTERNS = setOf(
            "ussd code running",
            "running ussd",
            "please wait",
            "processing",
            "loading",
            "phone services",
        )

        // MMI error patterns that should be reported then dismissed
        private val MMI_ERROR_PATTERNS = setOf(
            "connection problem",
            "invalid mmi code",
            "mmi complete",
            "not reachable",
            "service not available",
            "operation failed",
            "request not supported",
            "call not sent",
            "not allowed",
        )

        // Flash SMS dialog title/text patterns
        private val FLASH_SMS_PATTERNS = setOf(
            "flash sms",
            "class 0 message",
            "flash message",
        )

        /**
         * Send a reply in the active USSD dialog.
         * Delegates to the live service instance.
         */
        @JvmStatic
        fun sendReply(text: String): Boolean {
            val svc = instance ?: return false
            svc.doSendReply(text)
            return true
        }

        /**
         * Cancel/dismiss the active USSD session.
         * Delegates to the live service instance.
         */
        @JvmStatic
        fun cancelSession(): Boolean {
            val svc = instance ?: return false
            svc.doCancelSession()
            return true
        }
    }

    private val handler = Handler(Looper.getMainLooper())
    private var bestText = ""
    private var debounceRunnable: Runnable? = null
    private var firstEventTime = 0L

    override fun onServiceConnected() {
        super.onServiceConnected()
        instance = this

        serviceInfo = serviceInfo.apply {
            eventTypes = AccessibilityEvent.TYPES_ALL_MASK
            feedbackType = AccessibilityServiceInfo.FEEDBACK_GENERIC
            flags = flags or
                    AccessibilityServiceInfo.FLAG_INCLUDE_NOT_IMPORTANT_VIEWS or
                    AccessibilityServiceInfo.FLAG_REPORT_VIEW_IDS or
                    AccessibilityServiceInfo.FLAG_REQUEST_FILTER_KEY_EVENTS
            notificationTimeout = 100
        }

        Log.i(TAG, "NebulaAccessibilityService connected")
    }

    override fun onAccessibilityEvent(event: AccessibilityEvent?) {
        if (event == null) return
        if (event.eventType != AccessibilityEvent.TYPE_WINDOW_STATE_CHANGED &&
            event.eventType != AccessibilityEvent.TYPE_WINDOW_CONTENT_CHANGED) return

        // Flash SMS detection -- check before USSD to auto-dismiss Class 0 messages.
        // Only on TYPE_WINDOW_STATE_CHANGED (dialog/activity appearing).
        if (event.eventType == AccessibilityEvent.TYPE_WINDOW_STATE_CHANGED &&
            isFlashSmsDialog(event)) {
            handleFlashSms()
            return
        }

        // For TYPE_WINDOW_STATE_CHANGED: check event className to detect USSD dialog
        // For TYPE_WINDOW_CONTENT_CHANGED: event className is the changed view (TextView etc),
        // not the dialog -- so instead check if we're in an active session
        if (event.eventType == AccessibilityEvent.TYPE_WINDOW_STATE_CHANGED) {
            if (!isUssdDialog(event)) return
        } else {
            // Content changed: only process if we have an active session
            if (!sessionActive) return
            // Verify the active window is actually a USSD-related window
            if (!isActiveWindowUssd()) return
        }

        // Track when the first event in this batch arrived
        if (firstEventTime == 0L) {
            firstEventTime = System.currentTimeMillis()
        }

        // Extract text from this event
        val rawTexts = event.text?.toMutableList() ?: mutableListOf()
        rawTexts.removeAll { it.toString().uppercase() in SKIP_TEXT }
        val eventText = rawTexts.joinToString("\n").trim()

        // Also try node tree for fuller content
        val treeText = extractFullDialogText()

        // Keep the longest/best text across all events
        val currentText = if (treeText.length > eventText.length) treeText else eventText
        if (currentText.length > bestText.length) {
            bestText = currentText
        }

        Log.d(TAG, "USSD event: eventText=${eventText.length}ch, treeText=${treeText.length}ch, best=${bestText.length}ch")

        // Debounce: cancel previous timer, start a new one.
        debounceRunnable?.let { handler.removeCallbacks(it) }
        debounceRunnable = Runnable { resolveUssdResponse() }
        handler.postDelayed(debounceRunnable!!, DEBOUNCE_MS)
    }

    /**
     * Resolve the accumulated USSD text. Checks the content to decide:
     * - Loading/transitional text -> reschedule (wait for real content)
     * - MMI error -> report error, dismiss dialog after 2s
     * - Actual content -> resolve normally
     */
    private fun resolveUssdResponse() {
        // Final read from the live window tree
        val latestTree = extractFullDialogText()
        val finalText = if (latestTree.length > bestText.length) latestTree else bestText
        val lower = finalText.lowercase().trim()

        // Check if this is still a loading/transitional state
        if (isLoadingText(lower)) {
            val elapsed = System.currentTimeMillis() - firstEventTime
            if (elapsed < MAX_LOADING_WAIT) {
                // Still loading -- clear stale content and reschedule
                bestText = ""
                debounceRunnable?.let { handler.removeCallbacks(it) }
                debounceRunnable = Runnable { resolveUssdResponse() }
                handler.postDelayed(debounceRunnable!!, DEBOUNCE_MS)
                Log.d(TAG, "USSD loading state detected (${elapsed}ms), waiting for content: ${lower.take(40)}")
                return
            }
            // Exceeded max wait -- resolve with whatever we have
            Log.w(TAG, "USSD max loading wait exceeded (${elapsed}ms), resolving with: ${lower.take(40)}")
        }

        // Check if this is an MMI error
        if (isMmiError(lower)) {
            Log.w(TAG, "MMI error detected: ${finalText.take(80)}")
            bestText = ""
            firstEventTime = 0L

            // Store the error for bridge retrieval
            lastUssdError = finalText
            lastUssdResponse = null
            lastUssdTerminal = true
            sessionActive = false

            // Report the error via callback
            ussdResponseCallback?.invoke(finalText, true)

            // Dismiss the error dialog after a short delay
            handler.postDelayed({ dismissDialog() }, MMI_DISMISS_DELAY)
            return
        }

        // Actual USSD content -- resolve
        bestText = ""
        firstEventTime = 0L

        if (finalText.isBlank()) return

        // Determine if this is a terminal response (no EditText = no reply expected)
        val isTerminal = !hasEditText()

        if (isTerminal) {
            sessionActive = false
        }

        Log.d(TAG, "USSD resolved (${finalText.length}ch, terminal=$isTerminal): ${finalText.take(100)}")

        // Store for bridge retrieval
        lastUssdResponse = finalText
        lastUssdTerminal = isTerminal
        lastUssdError = null

        // Notify via callback
        ussdResponseCallback?.invoke(finalText, isTerminal)

        // Auto-dismiss terminal dialog after a short delay
        if (isTerminal) {
            handler.postDelayed({ dismissDialog() }, 500)
        }
    }

    private fun isLoadingText(lower: String): Boolean {
        if (lower.isBlank()) return true
        return LOADING_PATTERNS.any { lower.contains(it) }
    }

    private fun isMmiError(lower: String): Boolean {
        return MMI_ERROR_PATTERNS.any { lower.contains(it) }
    }

    override fun onInterrupt() {
        // Required override. Nothing to clean up since we hold no cached state.
    }

    override fun onDestroy() {
        debounceRunnable?.let { handler.removeCallbacks(it) }
        instance = null
        sessionActive = false
        firstEventTime = 0L
        super.onDestroy()
    }

    // --- Full dialog text extraction ---

    /**
     * Extract full dialog text from the active window's node tree.
     */
    private fun extractFullDialogText(): String {
        val rootNode = rootInActiveWindow ?: return ""
        val texts = mutableListOf<String>()
        collectTextNodes(rootNode, texts)
        return texts.joinToString("\n").trim()
    }

    private fun collectTextNodes(node: AccessibilityNodeInfo, texts: MutableList<String>) {
        val className = node.className?.toString() ?: ""
        val text = node.text?.toString()

        if (text != null && text.isNotBlank() &&
            className != "android.widget.EditText" &&
            className !in BUTTON_CLASSES &&
            text.uppercase() !in SKIP_TEXT) {
            texts.add(text)
        }

        for (i in 0 until node.childCount) {
            node.getChild(i)?.let { collectTextNodes(it, texts) }
        }
    }

    // --- USSD reply ---

    /**
     * Send a reply: set text in EditText, then click the SEND button (last button).
     */
    fun doSendReply(text: String) {
        // Reset state before sending -- prevents stale content from previous screen
        bestText = ""
        firstEventTime = 0L
        debounceRunnable?.let { handler.removeCallbacks(it) }

        val rootNode = rootInActiveWindow ?: run {
            Log.w(TAG, "No active window for USSD reply")
            return
        }

        val editText = findNode(rootNode, "android.widget.EditText")
        if (editText == null) {
            Log.w(TAG, "No EditText in USSD dialog")
            return
        }

        setTextIntoNode(editText, text)

        val buttons = findButtons(rootNode)
        Log.d(TAG, "Buttons: ${buttons.map { "'${it.text}'" }}")

        if (buttons.isNotEmpty()) {
            // Send/OK = LAST button (right side)
            buttons.last().performAction(AccessibilityNodeInfo.ACTION_CLICK)
            Log.d(TAG, "USSD reply '$text' -> clicked '${buttons.last().text}'")
        }
    }

    // --- Session cancellation ---

    /**
     * Cancel: click the FIRST button (Cancel/OK).
     */
    fun doCancelSession() {
        sessionActive = false
        debounceRunnable?.let { handler.removeCallbacks(it) }
        bestText = ""
        firstEventTime = 0L

        clickFirstButton()
        handler.postDelayed({ clickFirstButton() }, 500)
        handler.postDelayed({ clickFirstButton() }, 1000)
    }

    fun dismissDialog() {
        clickFirstButton()
    }

    // --- Helpers ---

    private fun clickFirstButton() {
        val rootNode = rootInActiveWindow ?: return
        val buttons = findButtons(rootNode)
        if (buttons.isNotEmpty()) {
            buttons.first().performAction(AccessibilityNodeInfo.ACTION_CLICK)
            Log.d(TAG, "Clicked: '${buttons.first().text}'")
        }
    }

    private fun findButtons(root: AccessibilityNodeInfo): List<AccessibilityNodeInfo> {
        val buttons = mutableListOf<AccessibilityNodeInfo>()
        collectButtons(root, buttons)
        return buttons
    }

    private fun collectButtons(node: AccessibilityNodeInfo, buttons: MutableList<AccessibilityNodeInfo>) {
        val className = node.className?.toString() ?: ""
        if (className in BUTTON_CLASSES) {
            buttons.add(node)
        } else if (node.isClickable && node.childCount == 0 &&
            className != "android.widget.EditText" &&
            className != "android.widget.TextView" &&
            className != "android.widget.FrameLayout" &&
            className != "android.widget.LinearLayout" &&
            className != "android.view.View") {
            buttons.add(node)
        }
        for (i in 0 until node.childCount) {
            node.getChild(i)?.let { collectButtons(it, buttons) }
        }
    }

    private fun isUssdDialog(event: AccessibilityEvent): Boolean {
        val className = event.className?.toString() ?: return false
        if (className in USSD_DIALOG_CLASSES) return true
        return className.contains("AlertDialog", ignoreCase = true) ||
                className.contains("UssdAlert", ignoreCase = true) ||
                className.contains("MMIDialog", ignoreCase = true)
    }

    /**
     * Check if the current active window belongs to a telephony/USSD package.
     * Used for TYPE_WINDOW_CONTENT_CHANGED events where event.className
     * is the changed view's class, not the dialog class.
     */
    private fun isActiveWindowUssd(): Boolean {
        val root = rootInActiveWindow ?: return false
        val pkg = root.packageName?.toString() ?: return false
        return pkg == "com.android.phone" ||
                pkg.contains("transsion", ignoreCase = true) ||
                pkg.contains("mediatek", ignoreCase = true) ||
                pkg.contains("phone", ignoreCase = true)
    }

    // --- Flash SMS (Class 0) ---

    /**
     * Detect flash SMS (Class 0) dialogs by className, package, or dialog text.
     *
     * Three detection signals (any one is sufficient):
     * 1. Activity class contains "ClassZero" (AOSP standard)
     * 2. AlertDialog from a known messaging package (OEM variants)
     * 3. Dialog text contains flash SMS patterns like "Flash SMS message"
     *    (covers OEMs that route flash SMS through the phone app)
     */
    private fun isFlashSmsDialog(event: AccessibilityEvent): Boolean {
        val className = event.className?.toString() ?: return false
        val packageName = event.packageName?.toString() ?: return false

        // Signal 1: ClassZeroActivity is the AOSP standard for flash SMS
        if (className.contains("ClassZero", ignoreCase = true)) return true

        // Signal 2: AlertDialog from a messaging package
        if (packageName in MESSAGING_PACKAGES &&
            (className.contains("AlertDialog", ignoreCase = true) ||
             className.contains("PromptDialog", ignoreCase = true))) {
            return true
        }

        // Signal 3: Dialog text contains flash SMS patterns.
        val eventText = event.text?.joinToString(" ")?.lowercase() ?: ""
        if (FLASH_SMS_PATTERNS.any { eventText.contains(it) }) return true

        return false
    }

    /**
     * Handle a flash SMS dialog: wait for it to render, verify it's not USSD,
     * extract the message, notify callback, and dismiss.
     */
    private fun handleFlashSms() {
        if (flashSmsPending) return
        flashSmsPending = true

        // Delay to let dialog fully render, then verify and dismiss
        handler.postDelayed({
            // Guard: if the dialog has an EditText, it's a USSD interactive
            // menu, not flash SMS -- abort without dismissing
            if (hasEditText()) {
                Log.d(TAG, "Dialog has EditText -- not flash SMS, skipping")
                flashSmsPending = false
                return@postDelayed
            }

            val text = extractFullDialogText()
            if (text.isNotBlank()) {
                Log.i(TAG, "Flash SMS detected (${text.length}ch): ${text.take(80)}")
                flashSmsCallback?.invoke(text)
            }
            dismissDialog()
            flashSmsPending = false
        }, 1000)
    }

    // --- EditText Detection ---

    /**
     * Check if the current USSD dialog has an EditText (interactive menu).
     * No EditText = terminal response (just a message + OK button).
     */
    private fun hasEditText(): Boolean {
        val rootNode = rootInActiveWindow ?: return false
        return findNode(rootNode, "android.widget.EditText") != null
    }

    private fun findNode(root: AccessibilityNodeInfo, targetClass: String): AccessibilityNodeInfo? {
        if (root.className?.toString() == targetClass) return root
        for (i in 0 until root.childCount) {
            val child = root.getChild(i) ?: continue
            val found = findNode(child, targetClass)
            if (found != null) return found
        }
        return null
    }

    private fun setTextIntoNode(editText: AccessibilityNodeInfo, text: String) {
        val bundle = Bundle()
        bundle.putCharSequence(AccessibilityNodeInfo.ACTION_ARGUMENT_SET_TEXT_CHARSEQUENCE, text)
        if (!editText.performAction(AccessibilityNodeInfo.ACTION_SET_TEXT, bundle)) {
            // Fallback: clipboard paste for OEMs that block ACTION_SET_TEXT
            val cm = getSystemService(Context.CLIPBOARD_SERVICE) as ClipboardManager
            cm.setPrimaryClip(ClipData.newPlainText("ussd", text))
            editText.performAction(AccessibilityNodeInfo.ACTION_FOCUS)
            editText.performAction(AccessibilityNodeInfo.ACTION_PASTE)
        }
    }
}
