@file:Suppress("MaxLineLength")
/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

package mozilla.appservices

import com.android.build.gradle.AppExtension
import com.android.build.gradle.LibraryExtension
import com.android.build.gradle.api.BaseVariant
import com.android.build.gradle.internal.api.TestedVariant
import org.gradle.api.GradleException
import org.gradle.api.Plugin
import org.gradle.api.Project
import org.gradle.api.artifacts.DependencySubstitutions
import org.gradle.api.artifacts.ModuleIdentifier
import org.gradle.api.artifacts.component.ModuleComponentIdentifier
import org.gradle.api.artifacts.component.ModuleComponentSelector
import org.gradle.api.internal.artifacts.DefaultModuleIdentifier
import org.gradle.api.logging.Logger
import org.gradle.api.logging.Logging

open class AppServicesPlugin : Plugin<Project> {
    internal lateinit var appServicesExtension: AppServicesExtension

    /**
     * Add substitutions for each component in the megazord.
     */
    internal fun DependencySubstitutions.substituteMegazord(megazord: MegazordDefinition, logger: Logger?) {
        this.all { dependency ->
            val requested = dependency.requested as? ModuleComponentSelector
            if (requested == null) {
                return@all
            }

            val identifier = megazord.components.find { it == requested.moduleIdentifier }
            if (identifier == null) {
                logger?.debug("substitution for '${requested.group}:${requested.module}' not found")
                return@all
            }

            val substitution = "${megazord.moduleIdentifier.group}:${megazord.moduleIdentifier.name}:${requested.version}"
            logger?.debug("substituting megazord module '$substitution' for component module" +
                    " '${requested.group}:${requested.module}:${requested.version}'")
            dependency.useTarget(substitution)
        }
    }

    /**
     * Turn "foo:bar" into "foo:bar-forUnitTests".
     */
    private fun ModuleIdentifier.forUnitTests(): ModuleIdentifier {
        return DefaultModuleIdentifier.newId(group, name + "-forUnitTests")
    }

    /**
     * Find the narrowest config matching the given variant, arbitrarily ordering product flavors ahead of build types.
     */
    private fun matchingConfig(variant: BaseVariant): VariantConfiguration {
        appServicesExtension.apply {
            return variants.filter { it.name == variant.name }
                    .plus(productFlavors.filter { variant.flavorName == it.name })
                    .plus(buildTypes.filter { variant.buildType.name == it.name })
                    .plus(defaultConfig)
                    .first()
        }
    }

    private fun configureVariant(project: Project, variant: TestedVariant) {
        val logger = Logging.getLogger("appservices")

        val config = matchingConfig(variant as BaseVariant)
        val megazordName = config.megazord

        if (megazordName == null) {
            logger.info("no megazord for variant ${variant.name}; not megazording")
            return
        }

        val megazord = appServicesExtension.megazords.findByName(megazordName)
        if (megazord == null) {
            throw GradleException("megazord named $megazordName not found configuring Android variant ${variant.name}")
        }

        listOf(variant.compileConfiguration, variant.runtimeConfiguration,
                // You'd think that the unit test variants would inherit from the underlying variants, but at the
                // crucial moment below they don't.  So we also megazord them.  We do this even when unit testing
                // is not enabled since that should make it easier to reason about the substitutions.
                variant.unitTestVariant.compileConfiguration, variant.unitTestVariant.runtimeConfiguration).forEach { configuration ->
            logger.info("substituting megazord $megazordName for variant ${variant.name}")
            configuration.resolutionStrategy.dependencySubstitution.substituteMegazord(megazord, logger)
        }

        if (config.unitTestingEnabled) {
            listOf(variant.unitTestVariant.compileConfiguration, variant.unitTestVariant.runtimeConfiguration).forEach { configuration ->
                // We've applied a megazord.  We need to add the corresponding `-forUnitTests` dependency...
                // but the versions must agree.  To find out the actual version chosen, we need to resolve the
                // configuration... but resolving the actual configuration at this time freezes it, which interacts
                // badly with many plugins and is generally Not A Good Thing for a plugin to do.
                // Therefore, we clone the configuration and resolve the clone to determine the actual megazord version,
                // and then use the same version.
                val modules: List<ModuleComponentIdentifier> = configuration.copyRecursive().incoming.resolutionResult.allComponents.mapNotNull { it.id as? ModuleComponentIdentifier }

                val resolvedMegazord = modules.find { it.group == megazord.moduleIdentifier.group && it.module == megazord.moduleIdentifier.name }
                if (resolvedMegazord == null) {
                    logger.error("megazord substitution requested for variant ${variant.name} but the megazord module" +
                            " '${megazord.moduleIdentifier.group}:${megazord.moduleIdentifier.name}' failed to resolve" +
                            " as part of the unit test $configuration!")
                    throw GradleException("megazord substitution for variant ${variant.name} failed to resolve a megazord module")
                }

                val forUnitTests = megazord.moduleIdentifier.forUnitTests()
                val dependency = "${forUnitTests.group}:${forUnitTests.name}:${resolvedMegazord.version}"
                logger.info("substituted megazord $megazordName for unit test $configuration;" +
                        " adding forUnitTests dependency '$dependency'")
                project.dependencies.add(configuration.name, dependency)
            }
        }
    }

    override fun apply(project: Project) {
        with(project) {
            appServicesExtension = extensions.create("appservices", AppServicesExtension::class.java, project)

            project.pluginManager.withPlugin("com.android.application") {
                var android = project.extensions.getByType(AppExtension::class.java)
                android.applicationVariants.all { variant ->
                    configureVariant(project, variant)
                }
            }

            // There's no reason that Android libraries shouldn't consume megazords, and it's helpful to add
            // forUnitTests dependencies.
            project.pluginManager.withPlugin("com.android.library") {
                //                var android = project.extensions.getByName("android") as LibraryExtension
                var android = project.extensions.getByType(LibraryExtension::class.java)
                android.libraryVariants.all { variant ->
                    configureVariant(project, variant)
                }
            }
        }
    }
}
