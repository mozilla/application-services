/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

{%- import "macros.kt" as kt %}
{%- let inner = self.inner() %}

{{ inner.doc()|comment("") }}
public class {{ inner.name()|class_name }}  {% call kt::render_constructor() %} : FMLFeatureInterface {
    {% call kt::render_class_body(inner) %}

    {%- if inner.has_prefs() %}
    override fun isModified(): Boolean =
        {% call kt::prefs() %}?.let { prefs ->
            listOf(
            {%- for p in inner.props() %}
            {%- if p.has_prefs() %}
                {{ p.pref_key().unwrap()|quoted }},
            {%- endif %}
            {%- endfor %}
            ).any { prefs.contains(it) }
        } ?: false
    {%- endif %}
}
