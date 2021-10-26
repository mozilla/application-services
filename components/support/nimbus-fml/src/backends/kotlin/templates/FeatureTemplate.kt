
{% import "macros.kt" as kt %}

{%- let inner = self.inner() %}
{%- let class_name = inner.name()|class_name -%}

public class {{class_name}}(

);
