{% import "macros.swift" as swift %}
{% let inner = self.inner() %}
{% let class_name = inner.name()|class_name %}
{{ inner.doc()|comment("") }}
public enum {{ class_name }}: String {
    {% for variant in inner.variants() %}
    {{ variant.doc()|comment("    ") }}
    case {{ variant.name()|enum_variant_name }} = {{variant.name()|quoted}}
    {% endfor %}

    public static func enumValue(_ s: String?) -> {{class_name}}? {
        if let s = s {
            return {{class_name}}(rawValue: s)
        }
        return nil
    }
}
