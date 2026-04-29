.PHONY: all build clean

rust_target := target/debug/manifest
cpp_target := cpp_manifest

all: build

build: $(cpp_target) $(rust_target) target/AllJavaFeatures.class bin/Debug/net10.0/linux-x64/main.dll bin/Debug/net10.0/linux-x64/main bin/Release/net10.0/linux-x64/publish/main target/main.kt.jar

$(cpp_target):
	g++ -std=c++23 -pthread src/main.cc src/run.cc -I src -o $(cpp_target)

$(rust_target):
	cargo build

target/AllJavaFeatures.class: AllJavaFeatures.java
	mkdir -p target
	javac -d target AllJavaFeatures.java

clean:
	rm -f $(cpp_target)
	rm -rf .zig-cache .zig-out
	cargo clean
	dotnet clean

bin/Release/net10.0/linux-x64/publish/main: main.cs
	dotnet publish -c Release -r linux-x64 /p:PublishAot=true

bin/Debug/net10.0/linux-x64/main.dll bin/Debug/net10.0/linux-x64/main: main.cs
	dotnet build -c Debug -r linux-x64

target/main.kt.jar: main.kt
	mkdir -p target
	kotlinc main.kt -include-runtime -d target/main.kt.jar
