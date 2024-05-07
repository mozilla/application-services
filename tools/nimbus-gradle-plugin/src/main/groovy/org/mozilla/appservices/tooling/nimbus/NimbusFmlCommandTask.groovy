/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

package org.mozilla.appservices.tooling.nimbus

import org.gradle.api.DefaultTask
import org.gradle.api.GradleException
import org.gradle.api.file.ProjectLayout
import org.gradle.api.file.RegularFileProperty
import org.gradle.api.provider.Property
import org.gradle.api.tasks.Input
import org.gradle.api.tasks.InputFiles
import org.gradle.api.tasks.Optional
import org.gradle.api.tasks.PathSensitive
import org.gradle.api.tasks.PathSensitivity
import org.gradle.api.tasks.TaskAction
import org.gradle.process.ExecOperations
import org.gradle.process.ExecSpec

import javax.inject.Inject

/**
 * A base task to execute a `nimbus-fml` command.
 *
 * Subclasses can declare additional inputs and outputs, and override
 * `configureFmlCommand` to set additional command arguments.
 *
 * This task requires either `applicationServicesDir` to be set, or
 * the `fmlBinary` to exist. If `applicationServicesDir` is set,
 * the task will run `nimbus-fml` from the Application Services repo;
 * otherwise, it'll fall back to a prebuilt `fmlBinary`.
 */
abstract class NimbusFmlCommandTask extends DefaultTask {
    public static final String APPSERVICES_FML_HOME = 'components/support/nimbus-fml'

    @Inject
    abstract ExecOperations getExecOperations()

    @Inject
    abstract ProjectLayout getProjectLayout()

    @Input
    abstract Property<String> getProjectDir()

    @Input
    @Optional
    abstract Property<String> getApplicationServicesDir()

    // `@InputFiles` instead of `@InputFile` because we don't want
    // the task to fail if the `fmlBinary` file doesn't exist
    // (https://github.com/gradle/gradle/issues/2016).
    @InputFiles
    @PathSensitive(PathSensitivity.NONE)
    abstract RegularFileProperty getFmlBinary()

    /**
     * Configures the `nimbus-fml` command for this task.
     *
     * This method is invoked from the `@TaskAction` during the execution phase,
     * and so has access to the final values of the inputs and outputs.
     *
     * @param spec The specification for the `nimbus-fml` command.
     */
    abstract void configureFmlCommand(ExecSpec spec)

    @TaskAction
    void execute() {
        execOperations.exec { spec ->
            spec.with {
                // Absolutize `projectDir`, so that we can resolve our paths
                // against it. If it's already absolute, it'll be used as-is.
                def projectDir = projectLayout.projectDirectory.dir(projectDir.get())
                def localAppServices = applicationServicesDir.getOrNull()
                if (localAppServices == null) {
                    if (!fmlBinary.get().asFile.exists()) {
                        throw new GradleException("`nimbus-fml` wasn't downloaded and `nimbus.applicationServicesDir` isn't set")
                    }
                    workingDir projectDir
                    commandLine fmlBinary.get().asFile
                } else {
                    def cargoManifest = projectDir.file("$localAppServices/$APPSERVICES_FML_HOME/Cargo.toml").asFile

                    commandLine 'cargo'
                    args 'run'
                    args '--manifest-path', cargoManifest
                    args '--'
                }
            }
            configureFmlCommand(spec)
        }
    }
}
