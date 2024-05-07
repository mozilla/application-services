/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

package org.mozilla.appservices.tooling.nimbus

import org.gradle.api.file.ConfigurableFileCollection
import org.gradle.api.file.DirectoryProperty
import org.gradle.api.file.RegularFileProperty
import org.gradle.api.provider.Property
import org.gradle.api.tasks.CacheableTask
import org.gradle.api.tasks.Input
import org.gradle.api.tasks.InputFile
import org.gradle.api.tasks.InputFiles
import org.gradle.api.tasks.LocalState
import org.gradle.api.tasks.OutputDirectory
import org.gradle.api.tasks.PathSensitive
import org.gradle.api.tasks.PathSensitivity
import org.gradle.process.ExecSpec

@CacheableTask
abstract class NimbusFeaturesTask extends NimbusFmlCommandTask {
    @InputFile
    @PathSensitive(PathSensitivity.RELATIVE)
    abstract RegularFileProperty getInputFile()

    @InputFiles
    @PathSensitive(PathSensitivity.RELATIVE)
    abstract ConfigurableFileCollection getRepoFiles()

    @Input
    abstract Property<String> getChannel()

    @LocalState
    abstract DirectoryProperty getCacheDir()

    @OutputDirectory
    abstract DirectoryProperty getOutputDir()

    @Override
    void configureFmlCommand(ExecSpec spec) {
        spec.with {
            args 'generate'

            args '--language', 'kotlin'
            args '--channel', channel.get()
            args '--cache-dir', cacheDir.get()
            for (File file : repoFiles) {
                args '--repo-file', file
            }

            args inputFile.get().asFile
            args outputDir.get().asFile
        }
    }
}
