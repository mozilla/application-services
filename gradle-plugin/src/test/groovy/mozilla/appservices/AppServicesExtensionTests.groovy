/* Any copyright is dedicated to the Public Domain.
 * http://creativecommons.org/publicdomain/zero/1.0/ */

package mozilla.appservices

import org.gradle.testkit.runner.GradleRunner
import org.junit.Rule
import org.junit.rules.TemporaryFolder
import spock.lang.Specification

class AppServicesExtensionTests extends Specification {
    @Rule final TemporaryFolder testProjectDir = new TemporaryFolder()
    File buildFile

    def setup() {
        buildFile = testProjectDir.newFile('build.gradle')
        testProjectDir.newFolder('src', 'main')
        File manifestFile = testProjectDir.newFile('src/main/AndroidManifest.xml')
        manifestFile << """
                <manifest xmlns:android="http://schemas.android.com/apk/res/android" package="org.mozilla.appservices.gradle-plugin.test">
                </manifest>
            """

        def sdkPath = System.getProperty('android.sdk.dir') ?: "/Users/nalexander/.mozbuild/android-sdk-macosx"
        File localProperties = testProjectDir.newFile('local.properties')
        localProperties << """
                sdk.dir=$sdkPath
        """

        buildFile << """
                buildscript {
                    repositories {
                        google()
                        jcenter()
                    }
                    dependencies {
                        classpath 'com.android.tools.build:gradle:3.1.4'
                    }
                }

                plugins {
                    id 'org.mozilla.appservices'
                }
        """
    }

    def androidLibrary() {
        buildFile << """
                // It's not possible to use the `plugins` block for the Android plugin yet.
                apply plugin: 'com.android.library'

                android {
                    compileSdkVersion 27

                    defaultConfig {
                        minSdkVersion 21
                        targetSdkVersion 27
                    }
                }

                repositories {
                    google()
                    jcenter()
                }
            """
    }

    def androidApplication() {
        buildFile << """
                // It's not possible to use the `plugins` block for the Android plugin yet.
                apply plugin: 'com.android.application'

                android {
                    compileSdkVersion 27

                    defaultConfig {
                        minSdkVersion 21
                        targetSdkVersion 27
                    }
                }

                repositories {
                    google()
                    jcenter()
                }
            """
    }

//    def setup() {
//        // XXX how to handle this?
//        SdkHandler.setTestSdkFolder(new File("/Users/nalexander/.mozbuild/android-sdk-macosx"))
//
//        testProjectDir.newFolder('src', 'main')
//        manifestFile = testProjectDir.newFile('src/main/AndroidManifest.xml')
//        manifestFile << """<manifest xmlns:android="http://schemas.android.com/apk/res/android" package="org.mozilla.appservices.megazord.test"></manifest>"""
//
//        project = ProjectBuilder.builder()
//                .withProjectDir(testProjectDir.root)
//                .build()
//
//        project.pluginManager.apply LibraryPlugin
//        project.android {
//            compileSdkVersion 27
//
//            defaultConfig {
//                minSdkVersion 21
//                targetSdkVersion 27
//            }
//        }
//
//        project.pluginManager.apply AppServicesPlugin
//    }

    def "megazording a library succeeds"() {
        given:
        androidLibrary()

        buildFile << """
            dependencies {
                // N.b.: there's a version 0.11.5, so this exercises the unit test version inspection.
                // If it was 0.11.5, taking the latest version might succeed by accident.
                implementation 'org.mozilla.places:places:0.11.4'
            }

            appservices {
                defaultConfig {
                    megazord = 'reference-browser'
                }
            }
        """

        when:
        def result = GradleRunner.create()
                .withProjectDir(testProjectDir.root)
                .withDebug(true)
                .withArguments('androidDependencies')
                .withPluginClasspath()
                .build()

        def sections = result.output.split("\n\n")

        then:
        sections.find { it.contains('debugCompileClasspath') }.contains('org.mozilla.appservices.composites:reference-browser:0.11.4@aar')
        !sections.find { it.contains('debugCompileClasspath') }.contains('org.mozilla.fxaclient:fxaclient:')
        !sections.find { it.contains('debugCompileClasspath') }.contains('org.mozilla.places:places:')
        !sections.find { it.contains('debugCompileClasspath') }.contains('org.mozilla.sync15:logins:')
        sections.find { it.contains('debugRuntimeClasspath') }.contains('org.mozilla.appservices.composites:reference-browser:0.11.4@aar')
        sections.find { it.contains('debugUnitTestCompileClasspath') }.contains('org.mozilla.appservices.composites:reference-browser:0.11.4@jar')
        !sections.find { it.contains('debugUnitTestCompileClasspath') }.contains('org.mozilla.fxaclient:fxaclient:')
        !sections.find { it.contains('debugUnitTestCompileClasspath') }.contains('org.mozilla.places:places:')
        !sections.find { it.contains('debugUnitTestCompileClasspath') }.contains('org.mozilla.sync15:logins:')
        sections.find { it.contains('debugUnitTestRuntimeClasspath') }.contains('org.mozilla.appservices.composites:reference-browser:0.11.4@jar')
    }

    def "megazording an application succeeds"() {
        given:
        androidApplication()

        buildFile << """
            dependencies {
                // N.b.: there's a version 0.11.5, so this exercises the unit test version inspection.
                // If it was 0.11.5, taking the latest version might succeed by accident.
                implementation 'org.mozilla.places:places:0.11.4'
            }

            appservices {
                defaultConfig {
                    megazord = 'reference-browser'
                }
            }
        """

        when:
        def result = GradleRunner.create()
                .withProjectDir(testProjectDir.root)
                .withDebug(true)
                .withArguments('androidDependencies')
                .withPluginClasspath()
                .build()

        def sections = result.output.split("\n\n")

        then:
        sections.find { it.contains('debugCompileClasspath') }.contains('org.mozilla.appservices.composites:reference-browser:0.11.4@aar')
        !sections.find { it.contains('debugCompileClasspath') }.contains('org.mozilla.fxaclient:fxaclient:')
        !sections.find { it.contains('debugCompileClasspath') }.contains('org.mozilla.places:places:')
        !sections.find { it.contains('debugCompileClasspath') }.contains('org.mozilla.sync15:logins:')
        sections.find { it.contains('debugRuntimeClasspath') }.contains('org.mozilla.appservices.composites:reference-browser:0.11.4@aar')
        sections.find { it.contains('debugUnitTestCompileClasspath') }.contains('org.mozilla.appservices.composites:reference-browser:0.11.4@jar')
        !sections.find { it.contains('debugUnitTestCompileClasspath') }.contains('org.mozilla.fxaclient:fxaclient:')
        !sections.find { it.contains('debugUnitTestCompileClasspath') }.contains('org.mozilla.places:places:')
        !sections.find { it.contains('debugUnitTestCompileClasspath') }.contains('org.mozilla.sync15:logins:')
        sections.find { it.contains('debugUnitTestRuntimeClasspath') }.contains('org.mozilla.appservices.composites:reference-browser:0.11.4@jar')
    }
}
