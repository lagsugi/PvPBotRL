# Third-party notices

## Files included in this repository

The Gradle Wrapper JARs, scripts and properties under `paper/` and `fabric/` come from the [Gradle Build Tool](https://github.com/gradle/gradle) and retain their upstream terms under the [Apache License, Version 2.0](LICENSES/Apache-2.0.txt). The scripts carry their original copyright notices and the JARs include a license copy. See [Gradle's licensing information](https://docs.gradle.org/current/userguide/licenses.html).

## Dependencies obtained separately

The standalone setup obtains Paper and Citizens from their upstream distribution sites. PvPBotRL loads Citizens as a separate runtime plugin. The setup verifies the pinned Citizens JAR by SHA-256 and installs it as published upstream. [Paper license](https://github.com/PaperMC/Paper/blob/main/LICENSE.md) · [Citizens build and download](tools/paper/setup.bat)

Citizens and CitizensAPI are available under OSL-3.0. Section 5 treats external deployment of Citizens or a derivative as distribution under section 1(c), including where third parties can use it over a network. Review the OSL terms and the source for the exact build when redistributing or externally deploying Citizens, especially if you modify it. [Citizens license](https://github.com/CitizensDev/Citizens2/blob/master/LICENSE) · [CitizensAPI license](https://github.com/CitizensDev/CitizensAPI/blob/master/LICENSE) · [OSL-3.0](https://opensource.org/license/OSL-3.0)

Build and research tools resolve their dependencies through the existing package managers and upstream download locations. Each dependency retains its own license.

Minecraft Java Edition, its game files, assets and generated decompiled sources are excluded from the project distribution. The Minecraft client and server are subject to the [Minecraft EULA](https://www.minecraft.net/en-us/eula) and [Usage Guidelines](https://www.minecraft.net/en-us/usage-guidelines).
