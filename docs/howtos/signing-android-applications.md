> Android requires that all APKs be digitally signed with a certificate before they can be installed.
[~ developer.android.com/studio/publish/app-signing](https://developer.android.com/studio/publish/app-signing)

To sign Android applications for release on Google Play you should use the [Autograph signing service](https://github.com/mozilla-services/autograph) from Mozilla services.

First provision an `AUTOGRAPH_EDGE_TOKEN` token by requesting one from the "Services Operations" team. When you get the token 
securely store it in your CI environment.
As part of your CI process you should build your application and then make a request to Autograph to sign the app.

## Circle CI example:

```sh
- run:
    name: Sign APK
    command: |
        curl -F "input=@/tmp/artifacts-android/app-release.apk" \
              -o /tmp/artifacts-android/app-release-signed.apk \
              -H "Authorization: $AUTOGRAPH_EDGE_TOKEN" \
              https://autograph-edge.prod.mozaws.net/sign
```

## Verify

You can verify that the application was correctly signed by using the `apksigner` tool:

```bash
- run:
    name: Verify APK
    command: |
        sudo apt update
        sudo apt install -y android-sdk-build-tools
        /opt/android/sdk/build-tools/27.0.3/apksigner verify --verbose /tmp/artifacts-android/app-release-signed.apk
```


You can find an example of this in the [Notes for Android](https://github.com/mozilla/notes/blob/ce9c0f2fa0f012d2fcdea204e4ea61f171db97f2/circle.yml) project.
