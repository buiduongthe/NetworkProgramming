// Bài mở rộng MR6.15 – app Android "Gọi thoại LTM" (Kotlin + Jetpack Compose). Mở thư mục này bằng Android Studio.
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
    }
}
rootProject.name = "GoiThoaiLTM"
include(":app")
