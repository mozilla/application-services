// This file was autogenerated by some hot garbage in the `nimbus-fml` crate.
// Trust me, you don't want to mess with it!

{%- match self.config.package_name() %}
{%- when Some with (package_name) %}
package {{ package_name }}
{%- else %}
{%- endmatch %}

import android.content.Context
import org.mozilla.experiments.nimbus.NullVariables
import org.mozilla.experiments.nimbus.Variables
import org.mozilla.experiments.nimbus.FeaturesInterface
import org.mozilla.experiments.nimbus.internal.FeatureHolder
import org.mozilla.experiments.nimbus.internal.mapValues
import org.mozilla.experiments.nimbus.internal.mapKeys
import org.mozilla.experiments.nimbus.internal.mapEntries
import org.mozilla.experiments.nimbus.internal.mergeWith

{%- for imported_class in self.imports() %}
import {{ imported_class }}
{%- endfor %}

{% let nimbus_object = self.config.nimbus_object_name() -%}
/**
 * An object for safely accessing feature configuration from Nimbus.
 *
 * This is generated.
 */
object {{ nimbus_object }} {
    class Features {
        {%- for f in self.iter_feature_defs() %}
        {%- let raw_name = f.name() %}
        {%- let class_name = raw_name|class_name %}
        {{ f.doc()|comment("        ") }}
        val {{raw_name|var_name}}: FeatureHolder<{{class_name}}> by lazy {
            FeatureHolder({ {{ nimbus_object }}.api }, {{ raw_name|quoted }}) { variables ->
                {{ class_name }}(variables)
            }
        }
        {%- endfor %}
    }

    /**
     * This should be populated at app launch.
     */
    var api: FeaturesInterface? = null

    val features = Features()
}

{%- for code in self.initialization_code() %}
{{ code }}
{%- endfor %}

// Public interface members begin here.
{% for code in self.declaration_code() %}
{{- code }}
{%- endfor %}

{% import "macros.kt" as kt %}