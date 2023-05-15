{%- import "macros.kt" as kt %}
{%- let inner = self.inner() %}

{{ inner.doc()|comment("") }}
public class {{ inner.name()|class_name }}  {% call kt::render_constructor() %} : FMLFeatureInterface {
    {% call kt::render_class_body(inner) %}
}