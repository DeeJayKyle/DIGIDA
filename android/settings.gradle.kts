pluginManagement {
    repositories {
        google()
        mavenCentral()
        gradlePluginPortal()
    }
}

dependencyResolutionManagement {
    repositoriesMode.set(RepositoriesMode.FAIL_ON_PROJECT_REPOS)
    repositories {
        google()
        mavenCentral()
        // The certificate verifier's Android half ships inside its Rust crate as a local
        // repository, found where cargo unpacked it.
        maven {
            url = uri(rustlsPlatformVerifier())
            metadataSources.artifact()
        }
    }
}

fun rustlsPlatformVerifier(): String {
    val metadata = providers.exec {
        workingDir = rootDir.parentFile
        commandLine("cargo", "metadata", "--format-version", "1", "--filter-platform", "aarch64-linux-android")
    }.standardOutput.asText.get()
    @Suppress("UNCHECKED_CAST")
    val packages = (groovy.json.JsonSlurper().parseText(metadata) as Map<String, Any>)["packages"] as List<Map<String, Any>>
    val manifest = packages.first { it["name"] == "rustls-platform-verifier-android" }["manifest_path"] as String
    return File(File(manifest).parentFile, "maven").path
}

rootProject.name = "DIGIDA"
include(":app")
