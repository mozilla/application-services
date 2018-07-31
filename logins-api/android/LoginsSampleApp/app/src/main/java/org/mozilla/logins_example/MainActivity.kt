package org.mozilla.logins_example

import android.Manifest
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
import android.widget.LinearLayout
import android.widget.TextView

import java.io.*

import com.beust.klaxon.Parser
import com.beust.klaxon.JsonObject

import org.mozilla.loginsapi.*

class MainActivity : AppCompatActivity() {
    var store: MentatLoginsStorage? = null;

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

        val recyclerView = findViewById<RecyclerView>(R.id.recycler_view).apply {
            setHasFixedSize(true)
            layoutManager = LinearLayoutManager(this@MainActivity)
            adapter = LoginViewAdapter(listOf<ServerPassword>())
        }

        fun refresh() {
            Log.d("TEST", "Refreshing logins...")
            Snackbar.make(recyclerView, "Loading logins...", Snackbar.LENGTH_LONG)
                    .setAction("Action", null).show()


            var whenStoreReady = if (this.store == null) {
                try {
                    initFromCredentials(ExampleApp.instance.account?.creds!!).then({ store ->
                        this.store = store;
                        SyncResult.fromValue(Unit)
                    }, { error ->
                        dumpError("LoginInit: ", error);
                        throw error
                    })
                } catch (e: MentatStorageException) {
                    return
                }
            } else {
                SyncResult.fromValue(Unit)
            }
            whenStoreReady.then({
                this.store!!.sync()
            }, { err ->
                dumpError("LoginSync: ", err);
                throw err;
            }).then({
                this.store!!.list();
            }, { err ->
                dumpError("LoginList: ", err);
                throw err;
            }).whenComplete {logins ->
                runOnUiThread {
                    (recyclerView.adapter as LoginViewAdapter).setLogins(logins)
                }
            }
        }

        fab.setOnClickListener { _ ->
            refresh()
        }
        refresh()
    }

    fun checkPermissions() {
        val permissionCheck = ContextCompat.checkSelfPermission(this, Manifest.permission.WRITE_EXTERNAL_STORAGE);

        if (permissionCheck == PackageManager.PERMISSION_GRANTED) {
            Log.d("LoginsSampleApp", "Got Permission!");
        } else {
            requestPermissions(arrayOf(Manifest.permission.WRITE_EXTERNAL_STORAGE), 1);
        }
    }

    fun initFromCredentials(creds: Credentials): SyncResult<MentatLoginsStorage> {
        // The format is a bit weird so I'm not sure if I can map this make klaxon do the
        // deserializing for us...
        val stringBuilder: StringBuilder = StringBuilder(creds.keys)
        val o = Parser().parse(stringBuilder) as JsonObject
        val info = o.obj("https://identity.mozilla.com/apps/oldsync")!!
        val appFiles = this.applicationContext.getExternalFilesDir(null)
        try {
            val file = File(appFiles.absolutePath + "/logins.mentatdb");
            if (file.exists()) {
                if (!file.delete()) {
                    Log.w("logins", "Failed to delete mentat db");
                } else {
                    Log.w("logins", "deleted mentat db");
                }
            }
        } catch(e: Exception) {
            e.printStackTrace();
        }

        val storage = MentatLoginsStorage(appFiles.absolutePath + "/logins.mentatdb");
        val unlockInfo = SyncUnlockInfo(
                kid = info.string("kid")!!,
                fxaAccessToken = creds.accessToken,
                syncKey = info.string("k")!!,
                tokenserverBaseURL = "https://token.services.mozilla.com"
        )

        return storage.unlock("my_secret_key", unlockInfo).then {
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

    // Replace the contents of a view (invoked by the layout manager)
    override fun onBindViewHolder(holder: ViewHolder, position: Int) {
        val p = logins[position]
        holder.v.apply {
            findViewById<TextView>(R.id.login_host).setText(p.hostname)
            findViewById<TextView>(R.id.login_username).setText(p.username)
        }
    }

    // Return the size of your dataset (invoked by the layout manager)
    override fun getItemCount() = logins.size
}
