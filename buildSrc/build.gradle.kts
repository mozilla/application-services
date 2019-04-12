/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

plugins {
    `kotlin-dsl`
}

// Use the Android Gradle plugin version from the root project so they stay in sync.
import java.util.Properties
fun readProperties(propertiesFile: File) = Properties().apply {
    propertiesFile.inputStream().use { fis ->
        load(fis)
    }
}
val properties = readProperties(File(rootDir.parentFile, "gradle.properties"))
val androidGradlePluginVersion = properties["androidGradlePluginVersion"]

dependencies {
    "implementation"("com.android.tools.build:gradle:$androidGradlePluginVersion")
}

repositories {
    google()
    jcenter()
}
