# Setting up the Fabric mod development environment

Set up a Windows/PowerShell development environment for Minecraft Java Edition **26.1.2**. The Fabric project's client instrumentation records input, movement, and packet application in an ordinary client for comparison with Rust and Paper. Use `runClient` for development and the `parityLaunch` task for the [isolated comparison workflow](../tools/parity/README.md#reproduce).

## 1. Versions

| Component | Version |
| --- | --- |
| Minecraft | 26.1.2 |
| JDK | 25 |
| Fabric Loader | 0.20.1 |
| Fabric API | 0.155.3+26.1.2 |
| Fabric Loom | 1.19.3 |
| Gradle | 9.8.2 |

The bundled Gradle Wrapper downloads Gradle. Initial dependency and game asset downloads require internet access.

## 2. Set up JDK 25

Install JDK 25, reopen PowerShell, and check the versions below. Use a development JDK with the compiler in addition to any Java runtime bundled with Minecraft Launcher.

```powershell
java -version
javac -version
```

Both commands should report version `25`.

If multiple Java installations are present, select the JDK explicitly. Replace the example path with your installation. These settings apply to the current PowerShell session.

```powershell
$env:JAVA_HOME = 'C:\Program Files\Eclipse Adoptium\jdk-25.0.5.8-hotspot'
$env:Path = "$env:JAVA_HOME\bin;$env:Path"
java -version
javac -version
```

## 3. Locate the project

Check out the repository and run the commands below from its root (containing `Cargo.toml` and `fabric/`). Paths to build products, sources, and game data in this guide are relative to `fabric/`.

```text
fabric/
├── build.gradle
├── settings.gradle
├── gradle.properties
├── gradlew
├── gradlew.bat
├── gradle/wrapper/
│   ├── gradle-wrapper.jar
│   └── gradle-wrapper.properties
└── src/
    ├── main/
    └── client/
```

`build/`, `.gradle/`, `.idea/`, and `run/` are generated during setup or execution and can be omitted when copying the project.

For a new project from scratch, the [official Fabric template's 26.1.2 branch](https://github.com/FabricMC/fabric-example-mod/tree/26.1.2) is a starting point. The mod name, package, and settings below are specific to this repository.

## 4. Check version settings

Check the settings in `gradle.properties`.

```properties
org.gradle.jvmargs=-Xmx2G
org.gradle.parallel=true
org.gradle.configuration-cache=true

minecraft_version=26.1.2
loader_version=0.20.1
loom_version=1.19.3
fabric_api_version=0.155.3+26.1.2

mod_version=0.1.0
maven_group=dev.sugi.pvpbot
archives_base_name=pvp-bot
```

`gradle/wrapper/gradle-wrapper.properties` specifies Gradle 9.8.2.

```properties
distributionUrl=https\://services.gradle.org/distributions/gradle-9.8.2-bin.zip
distributionSha256Sum=21bd5da2de49b642904077f38c0efdc7778e92b3ed0889392dca61b97e389607
```

The existing Wrapper configuration also includes timeouts and other settings. Preserve them when checking the two lines above.

This project uses the `net.fabricmc.fabric-loom` plugin. The Minecraft 26.1.2 configuration omits Yarn mappings. Loader and Fabric API dependencies are declared in `implementation`.

## 5. Verify Gradle and build

```powershell
.\fabric\gradlew.bat -p fabric --version
.\fabric\gradlew.bat -p fabric build
```

Confirm Gradle 9.8.2 and a Java 25 JVM. Allow time for downloads on the first run.

A successful build prints `BUILD SUCCESSFUL` and produces files in the following location.

```text
build/libs/pvp-bot-0.1.0.jar
build/libs/pvp-bot-0.1.0-sources.jar
```

On macOS/Linux, run `sh ./fabric/gradlew -p fabric build` from the repository root.

Install `pvp-bot-0.1.0.jar` into the game. `-sources.jar` is for browsing sources.

## 6. Generate Minecraft sources

```powershell
.\fabric\gradlew.bat -p fabric genSources
```

With the tested Loom/Gradle combination, specifying `build genSources` in one command produces a task dependency error. **Run the build and source generation separately.**

Generated Minecraft sources are saved in `*-sources.jar` under `.gradle/loom-cache/`. Place your mod code in `src/`.

## 7. Import into IntelliJ IDEA

1. Open the repository's `fabric/` folder in IntelliJ IDEA.
2. Import it as a Gradle project.
3. Set Project SDK to **JDK 25**.
4. Set Gradle JVM to **JDK 25** too.
5. Wait for Gradle synchronization to finish.

To generate IDE run configurations, run:

```powershell
.\fabric\gradlew.bat -p fabric ideaSyncTask
```

Launch the game using IntelliJ IDEA's `Minecraft Client` run configuration or Gradle's `runClient` task.

Ctrl-click a Minecraft class to open its generated source. If unavailable, run `genSources` and resync Gradle.

## 8. Launch the development client

```powershell
.\fabric\gradlew.bat -p fabric runClient
```

Check the launched game's log for mod initialization:

```text
PvP Bot initialized.
PvP Bot client initialized.
```

Development settings, worlds, and logs live in `run/`. Gradle launches the development game independently of a Fabric installation in Minecraft Launcher.

## 9. Code locations

| File or directory | Purpose |
| --- | --- |
| `src/main/java/dev/sugi/pvpbot/PvpBot.java` | Common mod initialization |
| `src/client/java/dev/sugi/pvpbot/client/PvpBotClient.java` | Client-only initialization |
| `src/main/resources/fabric.mod.json` | Mod ID, entry points, and dependencies |
| `gradle.properties` | Development environment and mod versions |

The mod ID is `lagsugi_pvpbotrl_parity`. Put code using client-only APIs such as screens, input, and rendering in `src/client/java/`.

The minimum Loader version in `fabric.mod.json` expands from `loader_version` at build time. Its current value is `>=0.20.1`.

## 10. Launch a dedicated server if needed

```powershell
.\fabric\gradlew.bat -p fabric runServer
```

The first launch may stop for EULA review. Read the terms and edit `run/eula.txt` only if you agree, then launch again.

## Troubleshooting

### Java missing or wrong version

Check `java -version`, `javac -version`, and `.\fabric\gradlew.bat -p fabric --version`. Select JDK 25 in both PowerShell and the IDE's Gradle JVM setting.

### Dependency download failure

Check internet connectivity and proxy settings, then retry. To refresh dependencies, run:

```powershell
.\fabric\gradlew.bat -p fabric build --refresh-dependencies
```

### Task dependency error during source generation and build

Run the commands separately:

```powershell
.\fabric\gradlew.bat -p fabric genSources
.\fabric\gradlew.bat -p fabric build
```

### Development game fails to launch

Check the PowerShell error and `run/logs/latest.log`. If you added mods, check compatibility with Minecraft 26.1.2 and the selected Loader.

## References

- [Fabric 26.1.2 development setup](https://docs.fabricmc.net/26.1.2/develop/getting-started/setting-up)
- [Fabric for Minecraft 26.1](https://fabricmc.net/2026/03/14/261.html)
- [Template supplying this project's Wrapper](https://github.com/FabricMC/fabric-example-mod/tree/10ea2384a5664a9b6c82d17e0fdb2773bbf6f1ba)

The Wrapper JAR came from the template above and was checked against Gradle's official SHA-256. Build, source generation, and IDE run configuration generation were verified for this project. Verify game launch on your own system.
