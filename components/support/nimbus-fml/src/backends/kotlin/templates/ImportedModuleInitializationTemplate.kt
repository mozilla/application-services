/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

{%- import "macros.kt" as kt %}
{%- let variables = "variables" %}
{%- let prefs = "prefs" %}
{%- let context = "variables.context" %}
{%- let class_name = self.inner.about().nimbus_object_name_kt() %}
        {{- class_name }}.features.apply {
            {%- for f in self.inner.features() %}
            {{ f.name()|var_name }}.withInitializer { {{ variables }}: Variables, {{ prefs }}: SharedPreferences? ->
                {{ f.name()|class_name }}(
                    _variables = {{ variables }},
                    _prefs = {{ prefs }}, {%- for p in f.props() %}
                    {{p.name()|var_name}} = {{ p.typ()|literal(self, p.default(), context) }}{% if !loop.last %},{% endif %}
                    {%- endfor %}
                )
            }
            {%- endfor %}
        }
