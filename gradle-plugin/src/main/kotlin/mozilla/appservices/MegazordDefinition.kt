/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

package mozilla.appservices

import org.gradle.api.GradleException
import org.gradle.api.artifacts.ModuleIdentifier
import org.gradle.api.internal.artifacts.DefaultModuleIdentifier

/**
 * A "megazord" is a named mapping from many component modules to a single megazord module.
 */
data class MegazordDefinition(val name: String, val components: MutableSet<ModuleIdentifier>) {
    constructor(name: String) : this(name, mutableSetOf())

    constructor(name: String, moduleIdentifier: ModuleIdentifier, components: Collection<ModuleIdentifier>?)
            : this(name, components?.toMutableSet() ?: mutableSetOf()) {
        this.moduleIdentifier = moduleIdentifier
    }

    lateinit var moduleIdentifier: ModuleIdentifier

    fun contains(component: ModuleIdentifier): Boolean {
        return this.components.contains(component)
    }

    private fun newId(identifier: String): ModuleIdentifier {
        val parts = identifier.split(':')
        if (parts.size != 2) {
            throw GradleException("megazord moduleIdentifier must have 2 colon-separated parts; got: '${identifier}' with ${parts.size} parts")
        }
        return DefaultModuleIdentifier.newId(parts[0], parts[1])
    }

    fun moduleIdentifier(identifier: String) {
        this.moduleIdentifier = newId(identifier)
    }

    fun moduleIdentifier(group: String, name: String) {
        this.moduleIdentifier = DefaultModuleIdentifier.newId(group, name)
    }

    fun component(identifier: String) {
        this.components.add(newId(identifier))
    }

    fun component(group: String, name: String) {
        this.components.add(DefaultModuleIdentifier.newId(group, name))
    }
}
