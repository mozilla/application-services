{%- import "macros.swift" as swift %}
{%- let inner = self.inner() %}
{%- let class_name = inner.name()|class_name -%}
{% call swift::render_class(inner) %}
extension {{ class_name }}: FMLFeatureInterface {}
