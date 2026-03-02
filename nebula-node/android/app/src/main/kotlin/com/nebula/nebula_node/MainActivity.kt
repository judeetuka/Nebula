package com.nebula.nebula_node

import android.app.Activity
import android.content.Intent
import com.nebula.nebula_node.platform.NebulaPlatformBridge
import com.nebula.nebula_node.platform.NebulaScreenCaptureService
import io.flutter.embedding.android.FlutterActivity
import io.flutter.embedding.engine.FlutterEngine

class MainActivity : FlutterActivity() {

    companion object {
        private const val REQUEST_MEDIA_PROJECTION = 4001
    }

    override fun configureFlutterEngine(flutterEngine: FlutterEngine) {
        super.configureFlutterEngine(flutterEngine)
        NebulaPlatformBridge.initialize(applicationContext)
    }

    /**
     * Handle the result from the MediaProjection consent dialog.
     * Stores the result code and data intent for NebulaScreenCaptureService to use.
     */
    override fun onActivityResult(requestCode: Int, resultCode: Int, data: Intent?) {
        super.onActivityResult(requestCode, resultCode, data)
        if (requestCode == REQUEST_MEDIA_PROJECTION) {
            NebulaScreenCaptureService.storeProjectionResult(resultCode, data)
        }
    }
}
