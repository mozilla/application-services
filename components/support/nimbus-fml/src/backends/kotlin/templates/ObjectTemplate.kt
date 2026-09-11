{%- import "macros.kt" as kt %}
{%- let inner = self.inner() %}
{%- let class_name = inner.name()|class_name %}

{{ inner.doc()|comment("") }}
public class {{ class_name }} {% call kt::render_constructor() %} : FMLObjectInterface {
    {% call kt::render_class_body(inner) %}

    internal fun _mergeWith(defaults: {{class_name}}?): {{class_name}} =
        defaults?.let { {{class_name}}(_variables = this._variables, _defaults = it._defaults) } ?: this

    companion object {
        internal fun create(variables: Variables): {{class_name}}? =
            {{class_name}}(_variables = variables)

        internal fun mergeWith(overrides: {{class_name}}, defaults: {{class_name}}): {{class_name}} =
            overrides._mergeWith(defaults)
    }
}
