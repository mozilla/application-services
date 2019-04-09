/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

package mozilla.appservices

import groovy.lang.Closure
import org.gradle.api.NamedDomainObjectContainer
import org.gradle.api.Project
import org.gradle.api.internal.artifacts.DefaultModuleIdentifier
import org.gradle.util.ConfigureUtil

// `AppServicesExtension` is documented in README.md.
open class AppServicesExtension(project: Project) {
    val megazords: NamedDomainObjectContainer<MegazordDefinition> = project.container(MegazordDefinition::class.java)

    fun megazords(configureClosure: Closure<*>): NamedDomainObjectContainer<MegazordDefinition> {
        return megazords.configure(configureClosure)
    }

    fun setMozillaMegazords() {
        megazords.clear()

        megazords.add(MegazordDefinition("lockbox",
                DefaultModuleIdentifier.newId("org.mozilla.appservices", "lockbox-megazord"),
                setOf(
                        DefaultModuleIdentifier.newId("org.mozilla.appservices", "fxaclient"),
                        DefaultModuleIdentifier.newId("org.mozilla.appservices", "logins"),
                        DefaultModuleIdentifier.newId("org.mozilla.appservices", "rustlog"),
                        DefaultModuleIdentifier.newId("org.mozilla.appservices", "viaduct")
                )))
        megazords.add(MegazordDefinition("reference-browser",
                DefaultModuleIdentifier.newId("org.mozilla.appservices", "reference-browser-megazord"),
                setOf(
                        DefaultModuleIdentifier.newId("org.mozilla.appservices", "fxaclient"),
                        DefaultModuleIdentifier.newId("org.mozilla.appservices", "logins"),
                        DefaultModuleIdentifier.newId("org.mozilla.appservices", "places"),
                        DefaultModuleIdentifier.newId("org.mozilla.appservices", "push"),
                        DefaultModuleIdentifier.newId("org.mozilla.appservices", "rustlog"),
                        DefaultModuleIdentifier.newId("org.mozilla.appservices", "viaduct")
                )))
        megazords.add(MegazordDefinition("fenix",
                DefaultModuleIdentifier.newId("org.mozilla.appservices", "fenix-megazord"),
                setOf(
                        DefaultModuleIdentifier.newId("org.mozilla.appservices", "fxaclient"),
                        DefaultModuleIdentifier.newId("org.mozilla.appservices", "places"),
                        DefaultModuleIdentifier.newId("org.mozilla.appservices", "push"),
                        DefaultModuleIdentifier.newId("org.mozilla.appservices", "rustlog"),
                        DefaultModuleIdentifier.newId("org.mozilla.appservices", "viaduct")
                )))
    }

    init {
        setMozillaMegazords()
    }

    val defaultConfig: VariantConfiguration = VariantConfiguration("defaultConfig")

    fun defaultConfig(configureClosure: Closure<*>): VariantConfiguration {
        val action = ConfigureUtil.configureUsing<VariantConfiguration>(configureClosure)
        action.execute(defaultConfig)

        return defaultConfig
    }

    val buildTypes: NamedDomainObjectContainer<VariantConfiguration> =
        project.container(VariantConfiguration::class.java)
    val productFlavors: NamedDomainObjectContainer<VariantConfiguration> =
        project.container(VariantConfiguration::class.java)
    val variants: NamedDomainObjectContainer<VariantConfiguration> =
        project.container(VariantConfiguration::class.java)

    fun buildTypes(configureClosure: Closure<*>): NamedDomainObjectContainer<VariantConfiguration> {
        return buildTypes.configure(configureClosure)
    }

    fun productFlavors(configureClosure: Closure<*>): NamedDomainObjectContainer<VariantConfiguration> {
        return productFlavors.configure(configureClosure)
    }

    fun variants(configureClosure: Closure<*>): NamedDomainObjectContainer<VariantConfiguration> {
        return variants.configure(configureClosure)
    }
}
