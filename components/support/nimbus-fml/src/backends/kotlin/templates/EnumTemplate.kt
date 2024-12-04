{% import "macros.kt" as kt %}
{% let inner = self.inner() %}
{% let class_name = inner.name()|class_name %}

{{ inner.doc()|comment("") }}
enum class {{class_name}} {
    {% for v in inner.variants() %}
    {{ v.doc()|comment("    ") }}
    {{ v.name()|enum_variant_name }}{% if !loop.last %},{% endif %}{% endfor %};

    companion object {
        private val enumMap: Map<String, {{class_name}}> by lazy {
            mapOf({% for v in inner.variants() %}
                {{ v.name()|quoted }} to {{class_name}}.{{ v.name()|enum_variant_name }}{% if !loop.last %},{% endif %}{% endfor %})
        }

        fun enumValue(string: String): {{class_name}}? = enumMap[string]
    }

    fun toJSONString() =
        when (this) {
            {%- for v in inner.variants() %}
            {{ v.name()|enum_variant_name }} -> {{ v.name()|quoted }}{% endfor %}
        }
}
