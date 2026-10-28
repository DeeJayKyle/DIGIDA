plugins {
    id("com.android.application")
    id("org.jetbrains.kotlin.android")
    id("org.jetbrains.kotlin.plugin.compose")
}

// The version follows the workspace's, handed in by CI as DIGIDA_VERSION, with the run
// number as the code so every build installs over the last.
val digidaVersion = System.getenv("DIGIDA_VERSION")?.removePrefix("v") ?: "1.0.0"
val digidaCode = System.getenv("GITHUB_RUN_NUMBER")?.toIntOrNull() ?: 1

android {
    namespace = "io.github.deejaykyle.digida"
    compileSdk = 35

    defaultConfig {
        applicationId = "io.github.deejaykyle.digida"
        minSdk = 26
        targetSdk = 35
        versionCode = digidaCode
        versionName = digidaVersion
        ndk {
            abiFilters += listOf("arm64-v8a", "x86_64")
        }
    }

    // A release key from the environment when CI has one, the debug key otherwise, so a build
    // without secrets still installs.
    val keystore = System.getenv("DIGIDA_KEYSTORE")?.let(::file)?.takeIf { it.exists() }
    signingConfigs {
        if (keystore != null) {
            create("release") {
                storeFile = keystore
                storePassword = System.getenv("DIGIDA_KEYSTORE_PASSWORD")
                keyAlias = System.getenv("DIGIDA_KEY_ALIAS")
                keyPassword = System.getenv("DIGIDA_KEY_PASSWORD")
            }
        }
    }

    buildTypes {
        release {
            isMinifyEnabled = true
            isShrinkResources = true
            proguardFiles(getDefaultProguardFile("proguard-android-optimize.txt"), "proguard-rules.pro")
            signingConfig = signingConfigs.findByName("release") ?: signingConfigs.getByName("debug")
        }
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }
    kotlinOptions {
        jvmTarget = "17"
    }
    buildFeatures {
        compose = true
    }
    packaging {
        jniLibs.useLegacyPackaging = false
    }
}

dependencies {
    val compose = platform("androidx.compose:compose-bom:2024.12.01")
    implementation(compose)
    implementation("androidx.compose.ui:ui")
    implementation("androidx.compose.foundation:foundation")
    implementation("androidx.compose.animation:animation")
    implementation("androidx.compose.material3:material3")
    implementation("androidx.compose.material:material-icons-extended")
    implementation("androidx.activity:activity-compose:1.9.3")
    implementation("androidx.core:core-ktx:1.15.0")
    implementation("androidx.lifecycle:lifecycle-runtime-compose:2.8.7")
    implementation("androidx.lifecycle:lifecycle-viewmodel-compose:2.8.7")
    implementation("androidx.media:media:1.7.0")
    implementation("androidx.browser:browser:1.8.0")
    implementation("io.coil-kt:coil-compose:2.7.0")
    implementation("org.jetbrains.kotlinx:kotlinx-coroutines-android:1.9.0")
    implementation("net.java.dev.jna:jna:5.15.0@aar")
}
