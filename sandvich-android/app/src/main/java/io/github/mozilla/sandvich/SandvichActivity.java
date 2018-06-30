package io.github.mozilla.sandvich;

import android.content.ComponentName;
import android.content.Intent;
import android.content.pm.ApplicationInfo;
import android.content.pm.PackageManager;
import android.net.Uri;
import android.support.customtabs.CustomTabsCallback;
import android.support.customtabs.CustomTabsClient;
import android.support.customtabs.CustomTabsIntent;
import android.support.customtabs.CustomTabsServiceConnection;
import android.support.customtabs.CustomTabsSession;
import android.support.v7.app.AppCompatActivity;
import android.os.Bundle;
import android.text.TextUtils;
import android.util.Log;
import android.view.View;
import android.widget.Button;
import android.widget.TextView;

import java.util.List;

import io.github.mozilla.sandvich.rust.Config;
import io.github.mozilla.sandvich.rust.FirefoxAccount;
import io.github.mozilla.sandvich.rust.OAuthInfo;
import io.github.mozilla.sandvich.rust.Profile;

import static android.provider.ContactsContract.Directory.PACKAGE_NAME;

public class SandvichActivity extends AppCompatActivity {
    static {
        System.loadLibrary("crypto");
        System.loadLibrary("ssl");
        System.loadLibrary("fxa_client");
    }

    CustomTabsServiceConnection customTabsConnection;

    // Globals
    final String contentBase = "https://sandvich-ios.dev.lcip.org";
    final String clientId = "98adfa37698f255b";
    private final String redirectUri =
            "lockbox://redirect.ios";

    private String flowUrl;
    public static final String CUSTOM_TAB_PACKAGE_NAME = "com.android.chrome";

    private FirefoxAccount fxa;

    @Override
    protected void onCreate(Bundle savedInstanceState) {
        super.onCreate(savedInstanceState);
        setContentView(R.layout.activity_sandvich);
        Button btn = findViewById(R.id.button);

        Config config = Config.custom(contentBase);

        this.fxa = new FirefoxAccount(config, clientId);
        String[] scopes = new String[] {"profile"};
        this.flowUrl = fxa.beginOAuthFlow(redirectUri, scopes, false);

        btn.setOnClickListener(new View.OnClickListener() {
            @Override
            public void onClick(View v) {
                openAuthTab(flowUrl);
            }
        });
    }

    @Override
    protected void onStart() {
        super.onStart();
        this.customTabsConnection = new CustomTabsServiceConnection() {
            @Override
            public void onCustomTabsServiceConnected(ComponentName name, CustomTabsClient client) {
                client.warmup(0L);
                CustomTabsSession session = client.newSession(null);

                session.mayLaunchUrl(Uri.parse(flowUrl), null, null);
            }

            @Override
            public void onServiceDisconnected(ComponentName name) {
                Log.i("onstart", "disconnect");
            }
        };
        CustomTabsClient.bindCustomTabsService(this, CUSTOM_TAB_PACKAGE_NAME, this.customTabsConnection);
    }

    @Override
    protected void onStop() {
        super.onStop();
        this.unbindService(this.customTabsConnection);
        this.customTabsConnection = null;
    }

    @Override
    protected void onNewIntent(Intent intent) {
        super.onNewIntent(intent);
        String action = intent.getAction();
        String data = intent.getDataString();

        if (Intent.ACTION_VIEW.equals(action) && data != null) {
            String info = authenticate(data);
            TextView txtView = findViewById(R.id.txtView);
            txtView.setText(info);
        }
    }

    private String authenticate(String redirectUrl) {
        Uri url = Uri.parse(redirectUrl);
        String code = url.getQueryParameter("code");
        String state = url.getQueryParameter("state");

        OAuthInfo oauthInfo = fxa.completeOAuthFlow(code, state);

        Profile profile = fxa.getProfile();
        return profile.email;
    }

    private void openAuthTab(String url) {
        CustomTabsIntent customTabsIntent = new CustomTabsIntent.Builder()
                .addDefaultShareMenuItem()
                .setShowTitle(true)
                .build();
        customTabsIntent.intent.setData(Uri.parse(flowUrl));
        PackageManager packageManager = getPackageManager();
        List<ApplicationInfo> resolveInfoList = packageManager.getInstalledApplications(PackageManager.GET_META_DATA);

        for (ApplicationInfo applicationInfo : resolveInfoList) {
            String packageName = applicationInfo.packageName;
            if (TextUtils.equals(packageName, PACKAGE_NAME)) {
                customTabsIntent.intent.setPackage(PACKAGE_NAME);
                break;
            }
        }
        customTabsIntent.launchUrl(SandvichActivity.this, Uri.parse(url));
    }
}
