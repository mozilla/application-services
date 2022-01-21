{%- import "macros.swift" as swift %}
{%- let inner = self.inner() %}
{%- let raw_name = inner.name() %}
{% let class_name = inner.name()|class_name -%}

{{ inner.doc()|comment("") }}
public class {{class_name}} {
    private var _variables: Variables
    var _defaults: Defaults
    init(_ _variables: Variables,_ _defaults: Defaults) {
        self._variables = _variables
        self._defaults = _defaults
    }
    {# The struct holds the default values that come from the manifest. They should completely
    specify all values needed for the  feature #}
    struct Defaults {
        {% for p in inner.props() %}
        {%- let t = p.typ() %}
        let {{p.name()|var_name}}: {{ t|type_label }}
    {%- endfor %}
    }

    {#- A constructor for application tests to use.  #}

    public convenience init(
        _variables: Variables = NilVariables.instance, {% for p in inner.props() %}
        {%- let t = p.typ() %}
        {{p.name()|var_name}}: {{ t|type_label }} = {{ t|literal(self, p.default()) }}{% if !loop.last %},{% endif %}
    {%- endfor %}
    ) {
        self.init(_variables, Defaults({% for p in inner.props() %}
        {%- let nm = p.name()|var_name %}{{ nm }}: {{ nm }}{% if !loop.last %}, {% endif %}
        {%- endfor %}))
    }

{#- The property initializers #}
{# -#}
    {% for p in inner.props() %}
    {%- let prop_swift = p.name()|var_name %}
    {{ p.doc()|comment("    ") }}
    public lazy var {{ prop_swift }}: {{ p.typ()|type_label }} = {
        {%- let defaults = format!("_defaults.{}", prop_swift) %}
        {{ p.typ()|property(p.name(), "_variables", defaults)}}
    }()
    {%- endfor %}

    func _mergeWith(_ defaults: {{class_name}}) -> {{class_name}} {
        return {{class_name}}(self._variables, defaults._defaults)
    }
    static func create(_ variables: Variables) -> {{class_name}} {
        return {{class_name}}(_variables: variables)
    }

    static func mergeWith(_ overrides: {{class_name}}, _ defaults: {{class_name}}) -> {{class_name}} {
            return overrides._mergeWith(defaults)
    }
}
