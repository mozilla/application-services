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
