/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

 {#- This file contains macros needed to generate code for the FML.

    It is the natural place to put commonalities between Object and Features,
    and rendering literals for Objects.
-#}

{% macro render_class_body(inner) %}
{%- let prop_name = inner.name()|var_name %}
{%- let raw_name = inner.name() -%}
   private constructor(
      private val _variables: Variables,
      private val _defaults: Defaults) {
   {# The data class holds the default values that come from the manifest. They should completely
   specify all values needed for the  feature #}
   private data class Defaults({% for p in inner.props() %}
      {%- let t = p.typ() %}
      val {{p.name()|var_name}}: {{ t|defaults_type_label }}{% if !loop.last %},{% endif %}
   {%- endfor %}
   )

   {#- A constructor for application tests to use.  #}

   constructor(_variables: Variables = NullVariables.instance, {% for p in inner.props() %}
   {%- let t = p.typ() %}
      {{p.name()|var_name}}: {{ t|defaults_type_label }} = {{ t|literal(self, p.default(), "_variables.context") }}{% if !loop.last %},{% endif %}
   {%- endfor %}
   ) : this(
      _variables = _variables,
      _defaults = Defaults({% for p in inner.props() %}
      {%- let nm = p.name()|var_name %}{{ nm }} = {{ nm }}{% if !loop.last %}, {% endif %}
      {%- endfor %})
   )

   {# The property initializers #}
   {%- for p in inner.props() %}
   {%- let prop_kt = p.name()|var_name %}
   {{ p.doc()|comment("    ") }}
   val {{ prop_kt }}: {{ p.typ()|type_label }} by lazy {
      {%- let defaults = format!("_defaults.{}", prop_kt) %}
      {{ p.typ()|property(p.name(), "_variables", defaults)}}
   }
{% endfor %}
{% endmacro %}