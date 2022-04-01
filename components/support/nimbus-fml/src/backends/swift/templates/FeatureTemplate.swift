{%- import "macros.swift" as swift %}
{%- let inner = self.inner() %}
{% call swift::render_class(inner) %}