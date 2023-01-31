// This file was autogenerated by the `nimbus-fml` crate.
// Trust me, you don't want to mess with it!

{%- match self.fm.about.nimbus_package_name() %}
{%- when Some with (package_name) %}
package {{ package_name }}

{% else -%}
{% endmatch %}
{%- for imported_class in self.imports() %}
import {{ imported_class }}
{%- endfor %}

{%- let nimbus_object = self.fm.about.nimbus_object_name_kt() %}

/**
 * An object for safely accessing feature configuration from Nimbus.
 *
 * This is generated.
 *
 * Before use to configure the application or any of its features, this class needs
 * to be wired up to the SDK API. This is an object created by the application which connects to
 * the Nimbus SDK and thence to the server.
 *
 * ```
 * val nimbus: Nimbus = connectToNimbusSDK()
 * {{ nimbus_object }}.initialize(getSdk = { nimbus })
 * ```
 *
 * Once initialized, this can be used to access typesafe configuration object via the `features` member.
 *
 * This class should not be edited manually, but changed by editing the `nimbus.fml.yaml` file, and
 * re-running the `nimbus-fml` tool, which is likely already being used by the build script.
 */
object {{ nimbus_object }} : FeatureManifestInterface<{{ nimbus_object }}.Features> {
    class Features {
        {%- for f in self.iter_feature_defs() %}
        {%- let raw_name = f.name() %}
        {%- let class_name = raw_name|class_name %}
        {{ f.doc()|comment("        ") }}
        val {{raw_name|var_name}}: FeatureHolder<{{class_name}}> by lazy {
            FeatureHolder({{ nimbus_object }}.getSdk, {{ raw_name|quoted }}) { variables ->
                {{ class_name }}(variables)
            }
        }
        {%- endfor %}
    }

    /**
     * This method should be called as early in the startup sequence of the app as possible.
     * This is to connect the Nimbus SDK (and thus server) with the `{{ nimbus_object }}`
     * class.
     *
     * The lambda MUST be threadsafe in its own right.
     */
    public override fun initialize(getSdk: () -> FeaturesInterface?) {
        this.getSdk = getSdk
        {%- for f in self.iter_feature_defs() %}
        this.features.{{- f.name()|var_name -}}.withSdk(getSdk)
        {%- endfor %}
        {%- for f in self.fm.iter_imported_files() %}
        {{ f.about().nimbus_object_name_kt() }}.initialize(getSdk)
        {%- endfor %}
        this.reinitialize()
    }

    private var getSdk: () -> FeaturesInterface? = {
        this.api
    }

    /**
     * This is the connection between the Nimbus SDK (and thus the Nimbus server) and the generated code.
     *
     * This is no longer the recommended way of doing this, and will be removed in future releases.
     *
     * The recommended method is to use the `initialize(getSdk)` method, much earlier in the application
     * startup process.
     */
    public var api: FeaturesInterface? = null

    /**
     * Refresh the cache of configuration objects.
     *
     * For performance reasons, the feature configurations are constructed once then cached.
     * This method is to clear that cache for all features configured with Nimbus.
     *
     * It must be called whenever the Nimbus SDK finishes the `applyPendingExperiments()` method.
     */
    public override fun invalidateCachedValues() {
        {%- for f in self.iter_feature_defs() %}
        features.{{- f.name()|var_name -}}.withCachedValue(null)
        {%- endfor %}
        {%- for f in self.fm.iter_imported_files() %}
        {{ f.about().nimbus_object_name_kt() }}.invalidateCachedValues()
        {%- endfor %}
    }

    /**
     * Accessor object for generated configuration classes extracted from Nimbus, with built-in
     * default values.
     */
    override val features = Features()

    {% let blocks = self.initialization_code() -%}
    /**
     * All generated initialization code. Clients shouldn't need to override or call
     * this.
     * We put it in a separate method because we have to be quite careful about what order
     * the initialization happens in— e.g. when importing other FML files.
     */
    private fun reinitialize() {
        {%- if !blocks.is_empty() %}
        {%- for code in blocks %}
        {{ code }}
        {%- endfor %}
        {%- else %}
        // Nothing left to do.
        {%- endif %}
    }

    {%- if !blocks.is_empty() %}

    init {
        this.reinitialize()
    }
    {%- endif %}
}

// Public interface members begin here.
{%- for code in self.declaration_code() %}
{{- code }}
{%- endfor %}

{% import "macros.kt" as kt %}