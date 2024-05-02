/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

package org.mozilla.appservices.tooling.nimbus

import org.gradle.api.Task
import org.gradle.api.provider.ListProperty

import java.util.stream.Collectors
import java.util.zip.ZipFile
import org.gradle.api.GradleException
import org.gradle.api.Plugin
import org.gradle.api.Project
import org.gradle.api.provider.MapProperty
import org.gradle.api.provider.Property
import org.gradle.api.tasks.Exec

abstract class NimbusPluginExtension {
    /**
     * The .fml.yaml manifest file.
     *
     * If absent this defaults to `nimbus.fml.yaml`.
     * If relative, it is relative to the project root.
     *
     * @return
     */
    abstract Property<String> getManifestFile()

    File getManifestFileActual(Project project) {
        var filename = this.manifestFile.getOrNull() ?: "nimbus.fml.yaml"
        return project.file(filename)
    }

    /**
     * The mapping between the build variant and the release channel.
     *
     * Variants that are not in this map are used literally.
     * @return
     */
    abstract MapProperty<String, String> getChannels()

    String getChannelActual(variant) {
        Map<String, String> channels = this.channels.get() ?: new HashMap()
        return channels.getOrDefault(variant.name, variant.name)
    }

    /**
     * The filename of the manifest ingested by Experimenter.
     *
     * If this is a relative name, it is taken to be relative to the project's root directory.
     *
     * If missing, this defaults to `.experimenter.json`.
     * @return
     */
    abstract Property<String> getExperimenterManifest()

    File getExperimenterManifestActual(Project project) {
        var filename = this.experimenterManifest.getOrNull() ?: ".experimenter.json"
        return project.file(filename)
    }

    /**
     * The directory to which the generated files should be written.
     *
     * This defaults to the generated sources folder in the build directory.
     *
     * @return
     */
    abstract Property<String> getOutputDir()

    File getOutputDirActual(Object variant, Project project) {
        var outputDir = this.outputDir.getOrNull() ?: "generated/source/nimbus/${variant.name}/kotlin"
        return project.layout.buildDirectory.dir(outputDir).get().asFile
    }

    /**
     * The file(s) containing the version(s)/ref(s)/location(s) for additional repositories.
     *
     * This defaults to an empty list.
     *
     * @return
     */
    abstract ListProperty<String> getRepoFiles()

    List<File> getRepoFilesActual(Project project) {
        var repoFiles = this.repoFiles.getOrNull() ?: new ArrayList<File>()
        return repoFiles.stream().map(filename -> {
            project.file(filename)
        }).collect(Collectors.toList())
    }

    /**
     * The directory where downloaded files are or where they should be cached.
     *
     * This defaults to `null`, in which case no cache directory will be used.
     *
     * @return
     */
    abstract Property<String> getCacheDir()

    File getCacheDirActual(Project project) {
        var cacheDir = this.cacheDir.getOrNull() ?: "nimbus-cache"
        return project.rootProject.layout.buildDirectory.dir(cacheDir).get().asFile
    }

    /**
     * The directory where a local installation of application services can be found.
     *
     * This defaults to `null`, in which case the plugin will download a copy of the correct
     * nimbus-fml binary for this version of the plugin.
     *
     * @return
     */
    abstract Property<String> getApplicationServicesDir()

    File getApplicationServicesDirActual(Project project) {
        var applicationServicesDir = this.applicationServicesDir.getOrNull()
        return applicationServicesDir ? project.file(applicationServicesDir) : null
    }
}

class NimbusPlugin implements Plugin<Project> {

    public static final String APPSERVICES_FML_HOME = "components/support/nimbus-fml"

    void apply(Project project) {
        def extension = project.extensions.create('nimbus', NimbusPluginExtension)

        Collection<Task> oneTimeTasks = new ArrayList<>()
        if (project.hasProperty("android")) {
            if (project.android.hasProperty('applicationVariants')) {
                project.android.applicationVariants.all { variant ->
                    setupVariantTasks(variant, project, extension, oneTimeTasks, false)
                }
            }

            if (project.android.hasProperty('libraryVariants')) {
                project.android.libraryVariants.all { variant ->
                    setupVariantTasks(variant, project, extension, oneTimeTasks, true)
                }
            }
        } else {
            setupVariantTasks([
                    name: project.name
            ],
            project, extension, oneTimeTasks, true)
        }
    }

    def setupAssembleNimbusTools(Project project, NimbusPluginExtension extension) {
        return project.task("assembleNimbusTools") {
            group "Nimbus"
            description "Fetch the Nimbus FML tools from Application Services"
            doLast {
                if (extension.getApplicationServicesDirActual(project) == null) {
                    fetchNimbusBinaries(project)
                } else {
                    println("Using local application services")
                }
            }
        }
    }

    // Try one or more hosts to download the given file.
    // Return the hostname that successfully downloaded, or null if none succeeded.
    static def tryDownload(File directory, String filename, String[] urlPrefixes) {
        return urlPrefixes.find { prefix ->
            def urlString = filename == null ? prefix : "$prefix/$filename"
            try {
                new URL(urlString).withInputStream { from ->
                    new File(directory, filename).withOutputStream { out ->
                        out << from;
                    }
                }
                true
            } catch (e) {
                false
            }
        }
    }

    // Fetches and extracts the pre-built nimbus-fml binaries
    def fetchNimbusBinaries(Project project) {
        def asVersion = getProjectVersion()

        def fmlPath = getFMLFile(project, asVersion)
        println("Checking fml binaries in $fmlPath")
        if (fmlPath.exists()) {
            println("nimbus-fml already exists at $fmlPath")
            return
        }

        def rootDirectory = getFMLRoot(project, asVersion)
        def archive = new File(rootDirectory, "nimbus-fml.zip")
        ensureDirExists(rootDirectory)

        if (!archive.exists()) {
            println("Downloading nimbus-fml cli version $asVersion")

            def successfulHost = tryDownload(archive.getParentFile(), archive.getName(),
                // Try archive.mozilla.org release first
                "https://archive.mozilla.org/pub/app-services/releases/$asVersion",
                // Try a github release next (TODO: remove this once we verify that publishing to
                // archive.mozilla.org is working).
                "https://github.com/mozilla/application-services/releases/download/v$asVersion",
                // Fall back to a nightly release
                "https://firefox-ci-tc.services.mozilla.com/api/index/v1/task/project.application-services.v2.nimbus-fml.$asVersion/artifacts/public/build"
            )

            if (successfulHost == null) {
                throw GradleException("Unable to download nimbus-fml tooling with version $asVersion.\n\nIf you are using a development version of the Nimbus Gradle Plugin, please set `applicationServicesDir` in your `build.gradle`'s nimbus block as the path to your local application services directory relative to your project's root.")
            } else {
                println("Downloaded nimbus-fml from $successfulHost")
            }

            // We get the checksum, although don't do anything with it yet;
            // Checking it here would be able to detect if the zip file was tampered with
            // in transit between here and the server.
            // It won't detect compromise of the CI server.
            tryDownload(rootDirectory, "nimbus-fml.sha256", successfulHost)
        }

        def archOs = getArchOs()
        println("Unzipping binary, looking for $archOs/nimbus-fml")
        def zipFile = new ZipFile(archive)
        zipFile.entries().findAll { entry ->
            return !entry.directory && entry.name.contains(archOs)
        }.each { entry ->
            fmlPath.withOutputStream { out ->
                def from = zipFile.getInputStream(entry)
                out << from
                from.close()
            }

            fmlPath.setExecutable(true)
        }
    }

    /**
     * The directory where nimbus-fml will live.
     * We put it in a build directory so we refresh it on a clean build.
     * @param project
     * @param version
     * @return
     */
    static File getFMLRoot(Project project, String version) {
        return project.layout.buildDirectory.dir("bin/nimbus/$version").get().asFile
    }

    static def getArchOs() {
        String osPart
        String os = System.getProperty("os.name").toLowerCase()
        if (os.contains("win")) {
            osPart = "pc-windows-gnu"
        } else if (os.contains("nix") || os.contains("nux") || os.contains("aix")) {
            osPart = "unknown-linux"
        } else if (os.contains("mac")) {
            osPart = "apple-darwin"
        } else {
            osPart = "unknown"
        }

        String arch = System.getProperty("os.arch").toLowerCase()
        String archPart
        if (arch.contains("x86_64")) {
            archPart = "x86_64"
        } else if (arch.contains("amd64")) {
            archPart = "x86_64"
        } else if (arch.contains("aarch")) {
            archPart = "aarch64"
        } else {
            archPart = "unknown"
        }
        println("OS and architecture detected as $os on $arch")
        return "${archPart}-${osPart}"
    }

    static File getFMLFile(Project project, String version) {
        String os = System.getProperty("os.name").toLowerCase()
        String binaryName = "nimbus-fml"
        if (os.contains("win")) {
            binaryName = "nimbus-fml.exe"
        }
        return new File(getFMLRoot(project, version), binaryName)
    }

    static String getFMLPath(Project project, String version) {
        return getFMLFile(project, version).getPath()
    }

    String getProjectVersion() {
        Properties props = new Properties()
        def stream = getClass().getResourceAsStream("/nimbus-gradle-plugin.properties")
        props.load(stream)
        return props.get("version")
    }

    def setupVariantTasks(variant, project, extension, oneTimeTasks, isLibrary = false) {
        def task = setupNimbusFeatureTasks(variant, project, extension)

        if (oneTimeTasks.isEmpty()) {
            // The extension doesn't seem to be ready until now, so we have this complicated
            // oneTimeTasks thing going on here. Ideally, we'd run this outside of this function.
            def assembleToolsTask = setupAssembleNimbusTools(project, extension)
            oneTimeTasks.add(assembleToolsTask)

            def validateTask = setupValidateTask(project, extension)
            validateTask.dependsOn(assembleToolsTask)
            oneTimeTasks.add(validateTask)
        }

        // Generating experimenter manifest is cheap, for now.
        // So we generate this every time.
        // In the future, we should try and make this an incremental task.
        oneTimeTasks.forEach {oneTimeTask ->
            if (oneTimeTask != null) {
                task.dependsOn(oneTimeTask)
            }
        }
    }

    def setupNimbusFeatureTasks(variant, project, extension) {
        String channel = extension.getChannelActual(variant)
        File inputFile = extension.getManifestFileActual(project)
        File outputDir = extension.getOutputDirActual(variant, project)
        File cacheDir = extension.getCacheDirActual(project)
        List<File> repoFiles = extension.getRepoFilesActual(project)

        var generateTask = project.task("nimbusFeatures${variant.name.capitalize()}", type: Exec) {
            description = "Generate Kotlin data classes for Nimbus enabled features"
            group = "Nimbus"

            doFirst {
                ensureDirExists(outputDir)
                if (cacheDir != null)
                    ensureDirExists(cacheDir)
                println("Nimbus FML generating Kotlin")
                println("manifest        $inputFile")
                println("cache dir       $cacheDir")
                println("repo file(s)    ${repoFiles.join(", ")}")
                println("channel         $channel")
            }

            doLast {
                println("outputFile    $outputDir")
            }

            def localAppServices = extension.getApplicationServicesDirActual(project)
            if (localAppServices == null) {
                workingDir project.rootDir
                commandLine getFMLPath(project, getProjectVersion())
            } else {
                def cargoManifest = new File(localAppServices, "$APPSERVICES_FML_HOME/Cargo.toml")

                commandLine "cargo"
                args "run"
                args "--manifest-path", cargoManifest
                args "--"
            }
            args "generate"
            args "--language", "kotlin"
            args "--channel", channel
            if (cacheDir != null)
                args "--cache-dir", cacheDir
            for (File file : repoFiles) {
                args "--repo-file", file
            }

            args inputFile
            args outputDir

            println args
        }

        if(variant.metaClass.respondsTo(variant, 'registerJavaGeneratingTask', Task, File)) {
            variant.registerJavaGeneratingTask(generateTask, outputDir)
        }

        def generateSourcesTask = project.tasks.findByName("generate${variant.name.capitalize()}Sources")
        if (generateSourcesTask != null) {
            generateSourcesTask.dependsOn(generateTask)
        } else {
            project.tasks.findAll().stream()
            .filter({ task ->
                return task.name.contains("compile")
            })
            .forEach({ task ->
                task.dependsOn(generateTask)
            })
        }

        return generateTask
    }

    def setupValidateTask(project, extension) {
        File inputFile = extension.getManifestFileActual(project)
        File cacheDir = extension.getCacheDirActual(project)
        List<File> repoFiles = extension.getRepoFilesActual(project)

        return project.task("nimbusValidate", type: Exec) {
            description = "Validate the Nimbus feature manifest for the app"
            group = "Nimbus"

            doFirst {
                if (cacheDir != null)
                    ensureDirExists(cacheDir)
                println("Nimbus FML: validating manifest")
                println("manifest             $inputFile")
                println("cache dir            $cacheDir")
                println("repo file(s)         ${repoFiles.join()}")
            }

            def localAppServices = extension.getApplicationServicesDirActual(project)
            if (localAppServices == null) {
                workingDir project.rootDir
                commandLine getFMLPath(project, getProjectVersion())
            } else {
                def cargoManifest = new File(localAppServices, "$APPSERVICES_FML_HOME/Cargo.toml")

                commandLine "cargo"
                args "run"
                args "--manifest-path", cargoManifest
                args "--"
            }
            args "validate"
            if (cacheDir != null)
                args "--cache-dir", cacheDir
            for (File file : repoFiles) {
                args "--repo-file", file
            }

            args inputFile
        }
    }

    static def ensureDirExists(File dir) {
        if (dir.exists()) {
            if (!dir.isDirectory()) {
                dir.delete()
                dir.mkdirs()
            }
        } else {
            dir.mkdirs()
        }
    }

    static def versionCompare(String versionA, String versionB) {
        def a = versionA.split("\\.", 3)
        def b = versionB.split("\\.", 3)
        for (i in 0..<a.length) {
            def na = Integer.parseInt(a[i])
            def nb = Integer.parseInt(b[i])
            if (na > nb) {
                return 1
            } else if (na < nb) {
                return -1
            }
        }
        return 0
    }
}
