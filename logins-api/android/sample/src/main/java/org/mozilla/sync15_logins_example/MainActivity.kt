/* Copyright 2018 Mozilla
 * Licensed under the Apache License, Version 2.0 (the "License"); you may not use
 * this file except in compliance with the License. You may obtain a copy of the
 * License at http://www.apache.org/licenses/LICENSE-2.0
 * Unless required by applicable law or agreed to in writing, software distributed
 * under the License is distributed on an "AS IS" BASIS, WITHOUT WARRANTIES OR
 * CONDITIONS OF ANY KIND, either express or implied. See the License for the
 * specific language governing permissions and limitations under the License. */
package org.mozilla.sync15_logins_example

import android.Manifest
import android.annotation.SuppressLint
import android.content.pm.PackageManager
import android.os.Bundle
import android.support.design.widget.Snackbar
import android.support.v4.content.ContextCompat
import android.support.v7.app.AppCompatActivity
import android.support.v7.widget.RecyclerView
import android.support.v7.widget.LinearLayoutManager
import android.util.Log
import android.view.LayoutInflater
import android.view.Menu
import android.view.MenuItem
import android.view.ViewGroup
import android.view.View
import android.widget.Button
import android.widget.LinearLayout
import android.widget.TextView

import java.io.*

import com.beust.klaxon.Parser
import com.beust.klaxon.JsonObject
import kotlinx.android.synthetic.main.activity_main.*;


import org.mozilla.sync15.logins.*
import java.util.*
import java.text.SimpleDateFormat

class MainActivity : AppCompatActivity() {
    private var store: MentatLoginsStorage? = null;
    private var recyclerView: RecyclerView? = null;

    fun dumpError(tag: String, e: Exception) {
        val sw = StringWriter();
        val pw = PrintWriter(sw);
        e.printStackTrace(pw);
        val stack = sw.toString();
        Log.e(tag, e.message);
        Log.e(tag, stack);
        // XXX - need to do something better on error.
        // this.editText.setText("rust error (${tag}): : ${e.message}\n\n${stack}\n");
    }

    fun whenStoreReady(): SyncResult<LoginsStorage> {
        return if (this.store != null) {
            SyncResult.fromValue(this.store as LoginsStorage)
        } else {
            initMentatStore().then({ store ->
                this.store = store;
                SyncResult.fromValue(store as LoginsStorage)
            }, { error ->
                dumpError("LoginInit: ", error);
                SyncResult.fromException(error)
            })
        }
    }

    fun refresh(sync: Boolean): SyncResult<Unit> {
        Log.d("TEST", "Refreshing logins...")

        if (sync) {
            Snackbar.make(recyclerView!!, "Loading logins...", Snackbar.LENGTH_LONG)
                    .setAction("Action", null).show()
        }

        return whenStoreReady().then { store ->
            if (sync) {
                store.sync(getUnlockInfo()).then { SyncResult.fromValue(store) }
            } else {
                SyncResult.fromValue(store)
            }
        }.then { store ->
            store.list()
        }.then({ SyncResult.fromValue(it) }) { err ->
            dumpError("LoginsMainActivity", err)
            SyncResult.fromException(err)
        }.then { logins ->
            runOnUiThread {
                (this.recyclerView!!.adapter as LoginViewAdapter).setLogins(logins)
            }
            SyncResult.fromValue(Unit)
        }
    }

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        setContentView(R.layout.activity_main)
        setSupportActionBar(toolbar)
        checkPermissions()

        val email = ExampleApp.instance.account?.email!!
        (findViewById(R.id.logged_in_as) as TextView).text = getString(R.string.logged_in_as, email)

        val button: View = findViewById(R.id.logout)
        button.setOnClickListener { view ->
            ExampleApp.instance.account = null
            ExampleApp.instance.startNewIntent()
            finish()
        }

        recyclerView = findViewById<RecyclerView>(R.id.recycler_view).apply {
            setHasFixedSize(true)
            layoutManager = LinearLayoutManager(this@MainActivity)
            adapter = LoginViewAdapter(listOf<ServerPassword>())
        }

        fab.setOnClickListener { _ ->
            refresh(true)
        }
        // Initially refresh without syncing to display the currently
        // downloaded data, but start a sync as soon as that finishes.
        refresh(false).whenComplete {
            refresh(true)
        }
    }
    fun refreshOnUiThread(sync: Boolean) {
        runOnUiThread {
            this@MainActivity.refresh(sync);
        }
    }

    fun checkPermissions() {
        val permissionCheck = ContextCompat.checkSelfPermission(this, Manifest.permission.WRITE_EXTERNAL_STORAGE);

        if (permissionCheck == PackageManager.PERMISSION_GRANTED) {
            Log.d("LoginsSampleApp", "Got Permission!");
        } else {
            requestPermissions(arrayOf(Manifest.permission.WRITE_EXTERNAL_STORAGE), 1);
        }
    }

    private fun getUnlockInfo(): SyncUnlockInfo {
        return credentialsToUnlockInfo(ExampleApp.instance.account?.creds!!)
    }

    private fun credentialsToUnlockInfo(creds: Credentials): SyncUnlockInfo {
        // The format is a bit weird so I'm not sure if I can map this make klaxon do the
        // deserializing for us...
        val stringBuilder: StringBuilder = StringBuilder(creds.keys)
        val o = Parser().parse(stringBuilder) as JsonObject
        val info = o.obj("https://identity.mozilla.com/apps/oldsync")!!

        return SyncUnlockInfo(
                kid = info.string("kid")!!,
                fxaAccessToken = creds.accessToken,
                syncKey = info.string("k")!!,
                tokenserverBaseURL = "https://token.services.mozilla.com"
        )
    }

    fun initMentatStore(): SyncResult<MentatLoginsStorage> {
        val appFiles = this.applicationContext.getExternalFilesDir(null)
        val storage = MentatLoginsStorage(appFiles.absolutePath + "/logins.mentatdb");
        return storage.unlock("my_secret_key").then {
            SyncResult.fromValue(storage)
        }
    }

    override fun onCreateOptionsMenu(menu: Menu): Boolean {
        // Inflate the menu; this adds items to the action bar if it is present.
        menuInflater.inflate(R.menu.menu_main, menu)
        return true
    }

    override fun onOptionsItemSelected(item: MenuItem): Boolean {
        // Handle action bar item clicks here. The action bar will
        // automatically handle clicks on the Home/Up button, so long
        // as you specify a parent activity in AndroidManifest.xml.
        return when (item.itemId) {
            R.id.action_settings -> true
            else -> super.onOptionsItemSelected(item)
        }
    }
}

class LoginViewAdapter(private var logins: List<ServerPassword>) :
        RecyclerView.Adapter<LoginViewAdapter.ViewHolder>() {

    // Provide a reference to the views for each data item
    // Complex data items may need more than one view per item, and
    // you provide access to all the views for a data item in a view holder.
    class ViewHolder(val v: LinearLayout) : RecyclerView.ViewHolder(v)

    fun setLogins(newLogins: List<ServerPassword>) {
        logins = newLogins
        notifyDataSetChanged()
    }

    // Create new views (invoked by the layout manager)
    override fun onCreateViewHolder(parent: ViewGroup,
                                    viewType: Int): LoginViewAdapter.ViewHolder {
        // create a new view
        val l = LayoutInflater.from(parent.context)
                 .inflate(R.layout.login_item, parent, false) as LinearLayout
        return ViewHolder(l)
    }

    @SuppressLint("SimpleDateFormat")
    private fun formatTimestamp(ts: Long): String {
        return SimpleDateFormat("yyyy-MM-dd'T'HH:mm:ss.SSSS").format(Date(ts))
    }

    // Replace the contents of a view (invoked by the layout manager).
    @SuppressLint("SetTextI18n") // Just a sample app so we don't care about i18n/l10n
    override fun onBindViewHolder(holder: ViewHolder, position: Int) {
        val p = logins[position]

        holder.v.apply {
            // Helper function to update storage and refresh the activity. Note that refreshing the
            // activity for a single item is overkill, and we should probably use LoginsStorage.get()
            // and RecyclerView.notifyItemChanged

            fun <T> mutateStorage(callback: (LoginsStorage) -> SyncResult<T>) {
                val activity = getContext() as MainActivity
                activity.whenStoreReady().then(callback) { err ->
                    activity.dumpError("LoginViewAdapter", err);
                    SyncResult.fromException(err)
                }.whenComplete {
                    activity.refreshOnUiThread(false)
                    Log.d("LoginViewAdapter", "Finished update");
                }
            }

            findViewById<TextView>(R.id.login_host).text = "Hostname: ${p.hostname}"
            findViewById<TextView>(R.id.login_form_http_info).text = if (p.formSubmitURL != null) {
                "Form Submit URL: ${p.formSubmitURL}"
            } else {
                "HTTP Realm: ${p.httpRealm}"
            }

            findViewById<TextView>(R.id.login_username).text = "Username: ${p.username}"
            findViewById<TextView>(R.id.login_password).text = "Password: ${p.password}"


            var usage = "Used ${p.timesUsed} times"
            if (p.timeLastUsed != 0L) {
                usage += ", last used at ${formatTimestamp(p.timeLastUsed)}"
            }

            findViewById<TextView>(R.id.login_usage).text = usage

            findViewById<TextView>(R.id.login_created_at).let { createdAtView ->
                if (p.timeCreated != 0L) {
                    createdAtView.visibility = View.VISIBLE;
                    createdAtView.text = "Created at ${formatTimestamp(p.timeCreated)}"
                } else {
                    createdAtView.visibility = View.GONE
                }
            }

            findViewById<TextView>(R.id.login_pass_changed_at).let { passChangedAtView ->
                if (p.timePasswordChanged != 0L) {
                    passChangedAtView.visibility = View.VISIBLE;
                    passChangedAtView.text = "Password last changed on ${formatTimestamp(p.timePasswordChanged)}"
                } else {
                    passChangedAtView.visibility = View.GONE
                }
            }

            val haveLoginFields = p.usernameField != null || p.passwordField != null

            // Hide the login fields if we don't have either
            findViewById<LinearLayout>(R.id.login_form_fields).visibility = if (haveLoginFields) View.VISIBLE else View.GONE
            if (haveLoginFields) {
                findViewById<TextView>(R.id.login_username_field).text = "Username Field: ${
                    if (p.usernameField != null && p.usernameField != "") p.usernameField  else "N/A"
                }";
                findViewById<TextView>(R.id.login_password_field).text = "Password Field: ${
                    if (p.passwordField != null && p.passwordField != "") p.passwordField  else "N/A"
                }"
            }


            findViewById<Button>(R.id.login_btn_delete).setOnClickListener {
                Log.d("LoginViewAdapter", "Deleting ServerPassword with id: " + p.id);
                mutateStorage { store -> store.delete(p.id) }
            }

            findViewById<Button>(R.id.login_btn_touch).setOnClickListener {
                Log.d("LoginViewAdapter", "Touching ServerPassword with id: " + p.id);
                mutateStorage { store -> store.touch(p.id) }
            }
        }
    }

    // Return the size of your dataset (invoked by the layout manager)
    override fun getItemCount() = logins.size
}
