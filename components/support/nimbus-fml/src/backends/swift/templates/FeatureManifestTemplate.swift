// This file was autogenerated by some hot garbage in the `nimbus-fml` crate.
// Trust me, you don't want to mess with it!

{%- for imported_module in self.imports() %}
#if canImport({{ imported_module }})
    import {{ imported_module }}
#endif
{%- endfor %}

{% let nimbus_object = self.fm.about.nimbus_object_name_swift() -%}
///
/// An object for safely accessing feature configuration from Nimbus.
///
/// This is generated.
public class {{ nimbus_object }} {
    ///
    /// This should be populated at app launch; this method of initializing features
    /// will be removed in favor of the `initialize` function.
    ///
    public var api: FeaturesInterface?

    ///
    /// This method should be called as early in the startup sequence of the app as possible.
    /// This is to connect the Nimbus SDK (and thus server) with the `{{ nimbus_object }}`
    /// class.
    ///
    /// The lambda MUST be threadsafe in its own right.
    public func initialize(with getSdk: @escaping () -> FeaturesInterface?) {
        self.getSdk = getSdk
        {%- for f in self.iter_feature_defs() %}
        self.features.{{- f.name()|var_name -}}.with(sdk: getSdk)
        {%- endfor %}
        {%- for f in self.fm.iter_imported_files() %}
        {{ f.about.nimbus_object_name_swift() }}.shared.initialize(with: getSdk)
        {%- endfor %}
    }

    fileprivate lazy var getSdk: GetSdk = { [self] in self.api }

    ///
    /// Represents all the features supported by Nimbus
    ///
    public let features = {{ nimbus_object }}Features()

    ///
    /// Refresh the cache of configuration objects.
    ///
    /// For performance reasons, the feature configurations are constructed once then cached.
    /// This method is to clear that cache for all features configured with Nimbus.
    ///
    /// It must be called whenever the Nimbus SDK finishes the `applyPendingExperiments()` method.
    ///
    public func invalidateCachedValues() {
        {%- for f in self.iter_feature_defs() %}
        features.{{- f.name()|var_name -}}.with(cachedValue: nil)
        {%- endfor %}
        {%- for f in self.fm.iter_imported_files() %}
        {{ f.about.nimbus_object_name_swift() }}.shared.invalidateCachedValues()
        {%- endfor %}
    }

    ///
    /// A singleton instance of {{ nimbus_object }}
    ///
    public static let shared = {{ nimbus_object }}()
}

public class {{ nimbus_object }}Features {
    {%- for f in self.iter_feature_defs() %}
    {%- let raw_name = f.name() %}
    {%- let class_name = raw_name|class_name %}
    {{ f.doc()|comment("        ") }}
    public lazy var {{raw_name|var_name}}: FeatureHolder<{{class_name}}> = {
        FeatureHolder({{ nimbus_object }}.shared.getSdk, featureId: {{ raw_name|quoted }}) { (variables) in
            {{ class_name }}(variables)
        }
    }()
    {%- endfor %}
}


{%- for code in self.initialization_code() %}
{{ code }}
{%- endfor %}

// Public interface members begin here.
{% for code in self.declaration_code() %}
{{- code }}
{%- endfor %}

{% import "macros.kt" as kt %}