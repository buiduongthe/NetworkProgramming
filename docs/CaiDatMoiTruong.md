# Cài đặt môi trường

Tài liệu này gom mọi lệnh cài công cụ và thư viện cần cho kho mã, theo thứ tự nên làm. Phần lớn bài chỉ dùng thư viện chuẩn của ngôn ngữ; các thành phần ghi "tùy chọn" chỉ cần cho một số bài (cột cuối). Phiên bản tối thiểu xem mục *Yêu cầu môi trường* trong [README](../README.md#yêu-cầu-môi-trường).

Sau khi cài, **đóng hết cửa sổ dòng lệnh rồi mở lại** để biến `PATH` nhận công cụ mới. Kiểm tra nhanh trên Windows: chạy `THU\00_KIEM_TRA_CAI_DAT.bat` (hoặc mục 0 của `THU_MENU.bat`) trong thư mục gốc của kho trên máy tác giả, hoặc gõ các lệnh ở cột "Kiểm tra".

## 1. Windows (winget)

`winget` có sẵn trên Windows 10 (1809+) và Windows 11. Mở **Command Prompt** hoặc **PowerShell**; khi Windows hỏi quyền quản trị thì đồng ý.

### 1.1. Năm ngôn ngữ

| Thành phần | Lệnh cài | Kiểm tra |
|---|---|---|
| Python 3.13 (có Tkinter) | `winget install -e --id Python.Python.3.13` | `py -3 --version` |
| JDK 21 | `winget install -e --id EclipseAdoptium.Temurin.21.JDK` | `java -version`, `javac -version` |
| .NET SDK 10 | `winget install Microsoft.DotNet.SDK.10` | `dotnet --version` |
| Go 1.24+ | `winget install -e --id GoLang.Go` | `go version` |
| Rust (rustup) | `winget install -e --id Rustlang.Rustup` | `cargo --version`, `rustc --version` |
| Trình liên kết C++ cho Rust (MSVC Build Tools) | `winget install -e --id Microsoft.VisualStudio.2022.BuildTools --override "--quiet --wait --add Microsoft.VisualStudio.Workload.VCTools --includeRecommended"` | `cargo build` của một bài Rust không báo thiếu `link.exe` |

- Rust trên Windows mặc định dùng toolchain MSVC nên **bắt buộc** có Build Tools (gói "Desktop development with C++"; tải khoảng 2–3 GB, cài 10–20 phút). Đã có Visual Studio 2022 với workload C++ thì bỏ qua.
- Lần biên dịch đầu của mỗi bài Rust tải crate từ crates.io (cần mạng), các bài có giao diện (egui, cpal) mất vài phút; các lần sau nhanh.

### 1.2. Công cụ dùng chung cho các bài có giao diện, âm thanh, video

| Thành phần | Lệnh cài | Dùng cho |
|---|---|---|
| ffmpeg (kèm ffplay) | `winget install -e --id Gyan.FFmpeg` | Camera thật, mẫu thử `testsrc`, phát **tệp video** (MR6.3, MR6.5 – mọi ngôn ngữ); micro/loa của bản Go (MR6.2, MR6.5) |
| gcc (MinGW-w64, cho cgo) | `winget install -e --id BrechtSanders.WinLibs.POSIX.UCRT` | Giao diện Fyne của bản Go (Bai5.3, MR5.3, MR5.6, MR6.2, MR6.3, MR6.5) |
| Bật cgo cho Go | `go env -w CGO_ENABLED=1` | như trên (chạy một lần sau khi có gcc) |
| Thư viện Fyne (Go) | trong thư mục bài Go có giao diện: `go -C gui mod tidy` | tải Fyne v2 và tạo `gui/go.sum` (cần mạng, một lần mỗi bài) |
| `sounddevice` (Python) | `py -3 -m pip install --user sounddevice` | Micro/loa thật của bản Python (MR6.2, MR6.5); không có thì tự dùng nguồn giả lập |

- Micro/loa của các bản khác không cần cài thêm: Java dùng `javax.sound` (có trong JDK), C# dùng `winmm` của Windows, Rust dùng WASAPI qua crate `cpal`.
- Cho phép ứng dụng dùng thiết bị: *Settings → Privacy & security → Camera* và *Microphone* → bật **Let desktop apps access …**.
- Mỗi webcam chỉ một chương trình mở được tại một thời điểm (DirectShow): khi thử nhiều client, chỉ một client chọn camera thật, các client còn lại chọn camera giả lập hoặc `testsrc`.
- Xem tên camera/micro mà ffmpeg thấy: `ffmpeg -hide_banner -list_devices true -f dshow -i dummy`.

### 1.3. Thành phần riêng của một số bài

| Thành phần | Lệnh cài | Dùng cho |
|---|---|---|
| `grpcio`, `grpcio-tools` (Python) | `py -3 -m pip install --user -r python\Chuong07\Bai7.5_gRPC\requirements.txt` | Bài 7.5 bản Python |
| `aiosmtpd` (Python) | `py -3 -m pip install --user aiosmtpd` | Bài 7.6 (máy chủ SMTP thử dùng chung cho cả 5 ngôn ngữ) |
| Maven | chưa có gói winget chính thức ổn định: tải bản nhị phân từ maven.apache.org, giải nén và thêm thư mục `bin` vào `PATH` (hoặc `choco install maven` nếu dùng Chocolatey) | Bài 7.5 bản Java (`mvn --version`) |
| Mosquitto (MQTT broker) | `winget install -e --id EclipseFoundation.Mosquitto` | Bài 7.8 |
| OpenSSL | `winget install -e --id ShiningLight.OpenSSL.Light` | Bài 7.6, 8.4 (tạo chứng chỉ thử) |
| Module Go của gRPC | trong `go\`: `go mod download` | Bài 7.5 bản Go |

ICMP thô (Bài 7.7) cần chạy dòng lệnh với quyền quản trị (*Run as administrator*).

## 2. Linux (Ubuntu/Debian)

```bash
sudo apt update
sudo apt install -y python3 python3-tk python3-pip openjdk-21-jdk ffmpeg \
     gcc libgl1-mesa-dev xorg-dev libasound2-dev pkg-config libportaudio2
# .NET 10: theo hướng dẫn của Microsoft cho bản phân phối (gói dotnet-sdk-10.0)
# Go 1.24+: tải từ go.dev/dl hoặc gói của bản phân phối nếu đủ mới
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh     # Rust
python3 -m pip install --user sounddevice                          # micro/loa thật (Python)
sudo apt install -y mosquitto openssl maven                        # tùy chọn: Bài 7.8, 7.6/8.4, 7.5 (Java)
```
- `libasound2-dev`, `pkg-config`: ALSA cho crate `cpal` (Rust); `gcc`, `libgl1-mesa-dev`, `xorg-dev`: Fyne (Go); `libportaudio2`: `sounddevice` (Python).
- Camera USB là `/dev/video0`; người dùng cần thuộc nhóm `video` (`sudo usermod -aG video $USER`, đăng nhập lại).
- Kịch bản giao diện tự động (`demo_gui`) chạy dưới `xvfb-run` (`sudo apt install -y xvfb imagemagick`).

## 3. macOS (Homebrew)

```bash
xcode-select --install                     # trình biên dịch C (cgo cho Fyne, crate Rust)
brew install python python-tk@3.13 openjdk@21 go rustup ffmpeg portaudio
brew install --cask dotnet-sdk                # .NET SDK
rustup-init                                # cài toolchain Rust
python3 -m pip install --user sounddevice
brew install mosquitto openssl maven       # tùy chọn
```
Lần đầu dùng camera/micro, macOS hỏi quyền cho ứng dụng Terminal – chọn *Allow* (hoặc *System Settings → Privacy & Security → Camera / Microphone*).

## 4. Chạy thử trên Windows bằng tệp .bat

Trên máy tác giả, thư mục gốc của kho có `THU_MENU.bat` (menu chọn bài × ngôn ngữ) và thư mục `THU\` (mỗi bài có giao diện/thiết bị thật một tệp cho từng ngôn ngữ, cùng `00_KIEM_TRA_CAI_DAT.bat`). Mỗi tệp tự kiểm tra công cụ (thiếu thì in lệnh cài như trên), biên dịch, mở các cửa sổ cần thiết và ghi nhật ký vào thư mục `_log`. Hai thư mục/tệp này không đưa lên GitHub (xem `.gitignore`). 

### 4.1. Chạy kịch bản và chụp kết quả trên Windows

`tools\demo_windows.py` chạy đúng kịch bản `demo.json` của từng bài trên Windows và lưu kết quả với hậu tố `_windows` (`ket_qua_windows.txt`, `ket_qua_windows.png`, `ket_qua_gui_windows.png`…), không đè kết quả Linux. Cách dùng xem mục *Tái lập kết quả thực thi* trong [README](../README.md#tái-lập-kết-quả-thực-thi). Cần thêm:

| Thành phần | Lệnh cài | Dùng cho |
|---|---|---|
| Pillow (Python) | `py -3 -m pip install --user pillow` | vẽ ảnh kết quả; chụp và ghép ảnh giao diện bản Python |
| Playwright + Chromium (Python) | `py -3 -m pip install --user playwright` rồi `py -3 -m playwright install chromium` | ảnh hoạt hình của MR1.2 (mọi ngôn ngữ) |
| curl | có sẵn (`curl.exe`, Windows 10 1803 trở lên) | Bài 7.2 |

Công cụ báo "BỎ QUA – thiếu …" kèm lệnh cài khi thiếu công cụ của bài (dotnet, javac, cargo, go, gcc, ffmpeg, mosquitto, openssl, protoc). Bản C# WinForms (thư mục `GiaoDien\`) chỉ chạy được trên Windows và cũng được công cụ chạy. `tools\demo_windows.ps1` cũ giờ chỉ gọi `tools\demo_windows.py` với cùng đối số.

Một số khác biệt hành vi giữa Windows và Linux thấy rõ trong `ket_qua_windows.txt` (là hành vi thật của hệ điều hành, không phải lỗi của bài):

- Kết nối TCP tới cổng không có chương trình nghe trên máy cục bộ: Linux trả RST ngay ("Connection refused"); Windows gửi lại SYN vài lần nên mất khoảng 2 s mới báo "actively refused" – với thời gian chờ ngắn hơn 2 s, bài báo "hết thời gian chờ". `localhost` thử `::1` trước rồi mới tới `127.0.0.1` nên cũng chậm thêm ~2 s khi máy chủ chỉ nghe IPv4.
- Đóng socket khi còn dữ liệu chưa đọc: Windows gửi RST, phía kia nhận "An existing connection was forcibly closed by the remote host" (10054) hoặc "aborted" (10053) thay vì kết thúc bình thường.
- UDP: sau khi gửi tới một cổng đã đóng, Windows báo lỗi 10054 ở lần `recvfrom` kế tiếp (gói ICMP "port unreachable"); các bài Python (MR6.2, MR6.3, MR6.5) bỏ qua `ConnectionResetError` ở vòng nhận.
- Không có tín hiệu SIGTERM: kịch bản MR6.1 gửi Ctrl+C (SIGINT) thay cho `kill -TERM`.
- Thời hạn đọc của socket (`SO_RCVTIMEO`) trên Windows thuộc riêng từng bản sao: bản sao tạo bằng `try_clone` (Rust) giữ thời hạn đã đặt lúc sao chép, gỡ ở socket gốc không gỡ được ở bản sao. Các bài Rust đặt và gỡ thời hạn trên đúng bản sao dùng để đọc; nếu không, kết nối im lặng quá 5 s bị coi là mất.
- Đồng hồ ngủ mặc định của Windows ~15,6 ms: các bài cần độ trễ dưới 1 ms xin độ phân giải 1 ms (`timeBeginPeriod`, bản C#, Rust) hoặc chờ bận phần cuối (bản Java).
- Java nhận tham số dòng lệnh theo bảng mã ANSI (cp1252) nên chữ Việt như "ở", "ượ" thành "?". Công cụ truyền lại nguyên văn qua biến môi trường `THAM_SO_UTF8` mà các chương trình Java nhận câu tiếng Việt (MR3.1, Bài 8.1, MR8.1, MR8.2) đọc; khi tự gõ lệnh, bật *Settings → Time & language → Language & region → Administrative language settings → Change system locale → Beta: Use Unicode UTF-8 for worldwide language support*.
- Multicast: Go đặt `IP_MULTICAST_LOOP = 0` khi nghe nhóm; trên Windows tùy chọn này áp dụng ở phía nhận nên Bài 6.6 bản Go bật lại để nhận gói của chương trình phát cùng máy.

Mục 7 của `CHAY_DEMO_WINDOWS.bat` cài một lượt các thư viện tùy chọn: `py -3 -m pip install --user pillow playwright aiosmtpd grpcio grpcio-tools`, `py -3 -m playwright install chromium`, `winget install -e --id EclipseFoundation.Mosquitto`.
