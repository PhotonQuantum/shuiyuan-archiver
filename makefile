help:
	@echo "osx|win|clean"
osx:
	cargo build --release --target x86_64-apple-darwin
	cargo build --release --target aarch64-apple-darwin
	mkdir -p dist/水源社区存档工具.app
	cp platforms/mac/* dist/水源社区存档工具.app/
	lipo -create -output dist/水源社区存档工具.app/shuiyuan-archiver target/x86_64-apple-darwin/release/shuiyuan-archiver target/aarch64-apple-darwin/release/shuiyuan-archiver
win:
	cargo build --release --target x86_64-pc-windows-msvc
	mkdir -p dist
	cp target/x86_64-pc-windows-msvc/release/shuiyuan-archiver.exe dist/水源社区存档工具.exe
clean:
	rm -rf dist
	cargo clean