/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

package org.mozilla.appservices.tooling.nimbus

import org.gradle.api.Action
import org.gradle.api.DefaultTask
import org.gradle.api.GradleException
import org.gradle.api.file.ArchiveOperations
import org.gradle.api.file.FileVisitDetails
import org.gradle.api.file.RegularFileProperty
import org.gradle.api.model.ObjectFactory
import org.gradle.api.provider.ListProperty
import org.gradle.api.provider.Property
import org.gradle.api.tasks.CacheableTask
import org.gradle.api.tasks.Input
import org.gradle.api.tasks.LocalState
import org.gradle.api.tasks.Nested
import org.gradle.api.tasks.OutputFile
import org.gradle.api.tasks.TaskAction

import javax.inject.Inject

import groovy.transform.Immutable

/**
 * A task that fetches a prebuilt `nimbus-fml` binary for the current platform.
 *
 * Prebuilt binaries for all platforms are packaged into ZIP archives, and
 * published to sources like `archive.mozilla.org` (for releases) or
 * TaskCluster (for nightly builds).
 *
 * This task takes a variable number of inputs: a list of archive sources,
 * and a list of glob patterns to find the binary for the current platform
 * in the archive.
 *
 * The unzipped binary is this task's only output. This output is then used as
 * an optional input to the `NimbusFmlCommandTask`s.
 */
@CacheableTask
abstract class NimbusAssembleToolsTask extends DefaultTask {
    @Inject
    abstract ArchiveOperations getArchiveOperations()

    @Nested
    abstract FetchSpec getFetchSpec()

    @Nested
    abstract UnzipSpec getUnzipSpec()

    /** The location of the fetched ZIP archive. */
    @LocalState
    abstract RegularFileProperty getArchiveFile()

    /**
     * The location of the fetched hash file, which contains the
     * archive's checksum.
     */
    @LocalState
    abstract RegularFileProperty getHashFile()

    /** The location of the unzipped binary. */
    @OutputFile
    abstract RegularFileProperty getFmlBinary()

    /**
     * Configures the task to download the archive.
     *
     * @param action The configuration action.
     */
    void fetch(Action<FetchSpec> action) {
        action.execute(fetchSpec)
    }

    /**
     * Configures the task to extract the binary from the archive.
     *
     * @param action The configuration action.
     */
    void unzip(Action<UnzipSpec> action) {
        action.execute(unzipSpec)
    }

    @TaskAction
    void assembleTools() {
        def sources = [fetchSpec, *fetchSpec.fallbackSources.get()].collect {
            new Source(new URI(it.archive.get()), new URI(it.hash.get()))
        }

        def successfulSource = sources.find { it.trySaveArchiveTo(archiveFile.get().asFile) }
        if (successfulSource == null) {
            throw new GradleException("Couldn't fetch archive from any of: ${sources*.archiveURI.collect { "`$it`" }.join(', ')}")
        }

        // We get the checksum, although don't do anything with it yet;
        // Checking it here would be able to detect if the zip file was tampered with
        // in transit between here and the server.
        // It won't detect compromise of the CI server.
        try {
            successfulSource.saveHashTo(hashFile.get().asFile)
        } catch (IOException e) {
            throw new GradleException("Couldn't fetch hash from `${successfulSource.hashURI}`", e)
        }

        def zipTree = archiveOperations.zipTree(archiveFile.get())
        def visitedFilePaths = []
        zipTree.matching {
            include unzipSpec.includePatterns.get()
        }.visit { FileVisitDetails details ->
            if (!details.directory) {
                if (visitedFilePaths.empty) {
                    details.copyTo(fmlBinary.get().asFile)
                    fmlBinary.get().asFile.setExecutable(true)
                }
                visitedFilePaths.add(details.relativePath)
            }
        }

        if (visitedFilePaths.empty) {
            throw new GradleException("Couldn't find any files in archive matching unzip spec: (${unzipSpec.includePatterns.get().collect { "`$it`" }.join(' | ')})")
        }

        if (visitedFilePaths.size() > 1) {
            throw new GradleException("Ambiguous unzip spec matched ${visitedFilePaths.size()} files in archive: ${visitedFilePaths.collect { "`$it`" }.join(', ')}")
        }
    }

    /**
     * Specifies the source from which to fetch the archive and
     * its hash file.
     */
    static abstract class FetchSpec extends SourceSpec {
        @Inject
        abstract ObjectFactory getObjectFactory()

        @Nested
        abstract ListProperty<SourceSpec> getFallbackSources()

        /**
         * Configures a fallback to try if the archive can't be fetched
         * from this source.
         *
         * The task will try fallbacks in the order in which they're
         * configured.
         *
         * @param action The configuration action.
         */
        void fallback(Action<SourceSpec> action) {
            def spec = objectFactory.newInstance(SourceSpec)
            action(spec)
            fallbackSources.add(spec)
        }
    }

    /** Specifies the URL of an archive and its hash file. */
    static abstract class SourceSpec {
        @Input
        abstract Property<String> getArchive()

        @Input
        abstract Property<String> getHash()
    }

    /**
     * Specifies which binary to extract from the fetched archive.
     *
     * The spec should only match one file in the archive. If the spec
     * matches multiple files in the archive, the task will fail.
     */
    static abstract class UnzipSpec {
        @Input
        abstract ListProperty<String> getIncludePatterns()

        /**
         * Includes all files whose paths match the pattern.
         *
         * @param pattern An Ant-style glob pattern.
         * @see org.gradle.api.tasks.util.PatternFilterable#include
         */
        void include(String pattern) {
            includePatterns.add(pattern)
        }
    }

    /** A helper to fetch an archive and its hash file. */
    @Immutable
    static class Source {
        URI archiveURI
        URI hashURI

        boolean trySaveArchiveTo(File destination) {
            try {
                saveURITo(archiveURI, destination)
                true
            } catch (IOException ignored) {
                false
            }
        }

        void saveHashTo(File destination) {
            saveURITo(hashURI, destination)
        }

        private static void saveURITo(URI source, File destination) {
            source.toURL().withInputStream { from ->
                destination.withOutputStream { out ->
                    out << from
                }
            }
        }
    }
}
