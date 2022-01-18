{% import "macros.swift" as swift %}
{% let inner = self.inner() %}
{% let class_name = inner.name()|class_name %}
{{ inner.doc()|comment("") }}
public enum {{ class_name }} {
    {% for variant in inner.variants() %}
    {{ variant.doc()|comment("    ") }}
    case {{ variant.name()|enum_variant_name }}
    {% endfor %}

    private static var enumMap: [String: {{ class_name }}] = {
        return [{% for v in inner.variants() %}
                {{v.name()|quoted}} : .{{v.name() | enum_variant_name}}{% if !loop.last %},{% endif %}{% endfor %}]
    }()

    static func enumValue(_ s: String) -> {{class_name}}? {
        return enumMap[s]
    }
}
