@echo off
setlocal EnableExtensions EnableDelayedExpansion

set "JAVA_HOME=C:\Program Files\Eclipse Adoptium\jdk-21.0.11.10-hotspot"
set "ANDROID_HOME=%LOCALAPPDATA%\Android\Sdk"
set "NDK_HOME=%ANDROID_HOME%\ndk\27.0.12077973"

if not exist "%JAVA_HOME%\bin\java.exe" (
  echo Expected Java was not found at "%JAVA_HOME%\bin\java.exe".
  exit /b 1
)

if not exist "%ANDROID_HOME%\cmdline-tools\latest\bin\sdkmanager.bat" (
  echo Android command-line tools were not found under "%ANDROID_HOME%".
  exit /b 1
)

if not exist "%NDK_HOME%" (
  echo Android NDK was not found at "%NDK_HOME%".
  exit /b 1
)

set "PATH=%JAVA_HOME%\bin;%ANDROID_HOME%\platform-tools;%ANDROID_HOME%\cmdline-tools\latest\bin;%ANDROID_HOME%\emulator;%PATH%"
set "Path=%PATH%"

call %*
exit /b %ERRORLEVEL%
