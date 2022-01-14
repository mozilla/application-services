{%- import "macros.kt" as kt %}
{%- let inner = self.inner() %}
{%- let prop_name = inner.name()|var_name %}
{%- let raw_name = inner.name() %}

{% let nimbus_object = self.nimbus_object_name() %}
{% let class_name = inner.name()|class_name -%}

{{ inner.doc()|comment("") }}
public class {{class_name}}
    private constructor(
        private val _context: Context,
        private val _variables: Variables? = null,
        private val _defaults: Defaults) {
{# The data class holds the default values that come from the manifest. They should completely
specify all values needed for the  feature #}
    data class Defaults({% for p in inner.props() %}
        {%- let t = p.typ() %}
        val {{p.name()|var_name}}: {{ t|type_label }} = {{ t|literal(self, p.default(), "_context") }}{% if !loop.last %},{% endif %}
    {%- endfor %}
    )
{# A constructor for Nimbus to use.  #}
    constructor(_context: Context?, _variables: Variables? = null) : this(
        _context,
        _variables,
        Defaults()
    )
{# A constructor for application tests to use.  #}
    constructor({% for p in inner.props() %}
        {%- let t = p.typ() %}
        {{p.name()|var_name}}: {{ t|type_label }}{% if !loop.last %},{% endif %}
    {%- endfor %}
    ) : this(
        _context: Context?,
        _variables = null,
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
    {%- endfor %}
}