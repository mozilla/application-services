package org.mozilla.logins_example

import android.support.v7.app.AppCompatActivity
import android.os.Bundle

class StartupActivity : AppCompatActivity() {

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        setContentView(R.layout.activity_startup)

        ExampleApp.instance.startNewIntent()
        finish()
    }
}
