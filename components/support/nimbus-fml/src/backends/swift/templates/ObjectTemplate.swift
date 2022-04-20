{%- import "macros.swift" as swift %}
{%- let inner = self.inner() %}
{% call swift::render_class(inner) -%}
{% let class_name = inner.name()|class_name -%}

public extension {{class_name}} {
    func _mergeWith(_ defaults: {{class_name}}?) -> {{class_name}} {
        if let defaults = defaults {
            return {{class_name}}(variables: self._variables, defaults: defaults._defaults)
        }
        // This will only happen in optional objects where their defaults is nil
        // no merging is required, we simply return this instance
        return self
    }

    static func create(_ variables: Variables?) -> {{class_name}}? {
        if let variables = variables {
            return {{class_name}}(variables)
        }
        return nil
    }

    static func mergeWith(_ overrides: {{class_name}}, _ defaults: {{class_name}}) -> {{class_name}} {
        return overrides._mergeWith(defaults)
    }
}
