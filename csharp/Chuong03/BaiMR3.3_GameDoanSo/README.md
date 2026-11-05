# Bài mở rộng MR3.3 – Trò chơi đoán số nhiều người chơi qua TCP (C#)

| Thông tin | Nội dung |
|---|---|
| Loại | **Bài mở rộng** (mini game) – không có trong giáo trình, không có mã giả tương ứng |
| Liên hệ với giáo trình | Chương 3: Bài 3.2 (định khung JSON có tiền tố độ dài), Bài 3.3 (giao thức yêu cầu/phản hồi); Chương 6: Bài 6.1 (server nhiều client) |
| Thư viện | .NET 10 BCL: `System.Net.Sockets`, `System.Text.Json.Nodes`, `BlockingCollection`, `OrderedDictionary<TKey, TValue>` |
| Bản ngôn ngữ khác | [Python](../../../python/Chuong03/BaiMR3.3_GameDoanSo/), [Go](../../../go/Chuong03/BaiMR3.3_GameDoanSo/), [Java](../../../java/Chuong03/BaiMR3.3_GameDoanSo/), [Rust](../../../rust/Chuong03/BaiMR3.3_GameDoanSo/) |

## Mục tiêu
Server chọn một số bí mật trong khoảng 1–100; người chơi lần lượt đoán, server trả lời *thấp quá*, *cao quá* hoặc *ĐÚNG*, và thu hẹp khoảng cho người kế tiếp. Trò chơi nhỏ nhưng đủ các vấn đề của một giao thức ứng dụng thật trên TCP:
- **Thiết kế giao thức yêu cầu/phản hồi**: mỗi thông điệp là một đối tượng JSON trong một khung `4 byte độ dài + nội dung` (kế thừa Bài 3.2), có trường `type` và các mã lỗi rõ ràng.
- **Máy trạng thái**: server ở `CHO` (chờ đủ người) → `DANG_CHOI` → kết thúc ván → `CHO`; client ở *chờ* → *đến lượt* → *kết thúc*. Mỗi thông điệp chỉ hợp lệ ở một số trạng thái.
- **Kiểm tra đầu vào ở server**: đúng lượt, giá trị là số nguyên, nằm trong khoảng, khung là JSON hợp lệ. Client cũng kiểm tra để báo sớm cho người dùng, nhưng **server không bao giờ tin client**.
- **Xử lý mất kết nối và hết giờ**: người chơi rời giữa chừng bị loại khỏi vòng lượt; người không trả lời trong thời hạn bị mất lượt.

## Kiến thức chính
- Thông điệp: client → server `hello {name}`, `guess {value}`, `bye`; server → client `welcome`, `start`, `turn {player, lo, hi}`, `result {player, value, hint}`, `timeout`, `left`, `error {code, msg}`, `game_over`, `bye`.
- Mã lỗi: `BAD_JSON`, `NEED_HELLO`, `BAD_NAME`, `NAME_TAKEN`, `GAME_IN_PROGRESS`, `GAME_NOT_STARTED`, `NOT_YOUR_TURN`, `BAD_VALUE`, `OUT_OF_RANGE`, `UNKNOWN_TYPE`. Lỗi không làm mất lượt; người chơi gửi lại được tới khi hết giờ.
- Kiến trúc **vòng lặp sự kiện một luồng**: mỗi kết nối có một luồng đọc chỉ nhận khung và đẩy sự kiện vào `BlockingCollection<GameEvent>`; một luồng trò chơi duy nhất giữ toàn bộ trạng thái và xử lý tuần tự, nên không cần khóa. Hết giờ của lượt chính là thời gian chờ của `TryTake(out ev, timeoutMs)`; hạn chót được kiểm tra cả trước khi chờ nên client gửi thông điệp liên tục cũng không “hoãn” được hết giờ.
- `Socket.NoDelay = true` (`TCP_NODELAY`): tắt thuật toán Nagle để các khung nhỏ đi ngay (Go bật sẵn tùy chọn này cho mọi kết nối TCP, .NET thì không).
- Bộ sinh số giả ngẫu nhiên LCG cùng công thức với bản Python, Go: cùng `--seed` cho cùng số bí mật ở mọi bản.

## Tệp
- `BaiMR3.3_GameDoanSo.csproj` – dự án console `net10.0`; vai trò `server` hoặc `choi`.
- `Program.cs` – chọn vai trò.
- `GiaoThuc.cs` – định khung (`RecvExact`, `SendFrame`, `RecvFrame`, `SendJson`, `DecodeJson`), giới hạn khung 64 KiB, kiểm tra kiểu số nguyên `TryInt`, `Repr` (in giá trị như `repr()` của Python), bộ sinh `Lcg`.
- `ServerDoanSo.cs` – server: luồng chấp nhận, luồng đọc, lớp `Game` (máy trạng thái, kiểm tra, phát tin) và vòng sự kiện.
- `NguoiChoi.cs` – client: chế độ người chơi (bàn phím) và bốn bot `chia_doi`, `ngau_nhien`, `nghich`, `im_lang`.
- `demo.json` – kịch bản chạy minh họa (dùng với `tools/demo.py`); người chơi Em là client bản Python (chạy chéo).
- `ket_qua.txt`, `ket_qua.png` – kết quả thực thi thật.

## Cách chạy
Trong thư mục bài (lệnh giống nhau trên Windows và Linux):
```bash
dotnet build -c Release
dotnet bin/Release/net10.0/BaiMR3.3_GameDoanSo.dll server 0.0.0.0 9000 --so-nguoi 2                 # cửa sổ 1
dotnet bin/Release/net10.0/BaiMR3.3_GameDoanSo.dll choi An 127.0.0.1 9000                           # cửa sổ 2: bạn tự chơi
dotnet bin/Release/net10.0/BaiMR3.3_GameDoanSo.dll choi Binh 127.0.0.1 9000 --bot chia_doi          # cửa sổ 3: đấu với bot
```
Có thể thay `dotnet bin/Release/net10.0/BaiMR3.3_GameDoanSo.dll` bằng `dotnet run --`. Khi đến lượt, chương trình in `Nhập số:`; Ctrl+D (Linux) hoặc Ctrl+Z, Enter (Windows) để rời trò chơi. Tùy chọn server: `--het-gio-ms` (mặc định 30000), `--seed`, `--min`, `--max`, `--so-van` (dừng sau số ván, 0 = chơi mãi). Mỗi ván là một phiên: hết ván server đóng mọi kết nối và chờ người chơi mới.

Tái lập kết quả từ thư mục gốc của kho: `python3 tools/demo.py csharp/Chuong03/BaiMR3.3_GameDoanSo` (cổng 42330).

## Kết quả thực thi
![Kết quả thực thi](ket_qua.png)

Server cần 4 người, mỗi lượt tối đa 1500 ms, `--seed 2026` cho số bí mật 39. Thứ tự lượt theo thứ tự vào phòng: Binh, Cuong, An, Dung.
- Cuong (bot *nghịch*) đoán khi ván chưa bắt đầu (`GAME_NOT_STARTED`), gửi một khung không phải JSON (`BAD_JSON`), đoán khi đang là lượt của Binh (`NOT_YOUR_TURN`), rồi đến lượt mình gửi chuỗi `"năm mươi"` (`BAD_VALUE`) và số 99 ngoài khoảng 1–49 (`OUT_OF_RANGE`) trước khi đoán hợp lệ 25. Các lỗi không làm mất lượt.
- An là người chơi nhập từ bàn phím (trong demo đầu vào lấy từ tệp): dòng `abc` bị chính client từ chối, không gửi đi.
- Dung (bot *im lặng*) không trả lời: sau 1500 ms server báo hết giờ, chuyển lượt; Dung ngắt kết nối và server loại Dung khỏi vòng lượt (còn 3 người).
- Em đến khi ván đang chơi nên bị từ chối (`GAME_IN_PROGRESS`) và đóng phiên.
- Binh tìm kiếm nhị phân (50, 32); khoảng được thu hẹp dần 1–100 → 1–49 → 26–49 → 26–39 → 33–39 → 37–39; An đoán 39 ở lượt 7 và thắng. Bản C# cho cùng trình tự sự kiện, từng dòng giống hệt bản Python (đã so bằng `diff` sau khi che số cổng). Em là client bản Python nối vào server C# – chạy chéo được vì cùng khung 4 byte + JSON UTF-8.

## Ghi chú
- Gửi thông báo cho mọi người chơi (`broadcast`) được làm ngay trong luồng trò chơi bằng `sendall`; một client nhận quá chậm có thể làm server chờ. Server thật cần hàng đợi gửi riêng cho từng client (xem Bài 6.4) hoặc đóng kết nối quá chậm.
- Kiểm tra kiểu: JSON `50.0`, `true`, `"50"` đều bị coi là `BAD_VALUE` (bản C# kiểm tra `GetValueKind() == JsonValueKind.Number` rồi `TryGetValue<long>`, nên `50.0`, `1e2` cũng bị loại như bản Python).
- Giới hạn khung 64 KiB (thay cho 1 MiB của Bài 3.2) vì thông điệp trò chơi chỉ vài chục byte: giới hạn càng sát nhu cầu, client xấu càng khó làm server tốn bộ nhớ.
- Thứ tự vào phòng quyết định thứ tự lượt, nên bản C# giữ người chơi trong `OrderedDictionary<Socket, string>` (.NET 9+): `Dictionary` thường không bảo đảm thứ tự liệt kê sau khi đã xóa phần tử.
- Mỗi lần phát tin, `JsonObject` được `DeepClone()` cho từng người nhận vì một nút JSON chỉ thuộc về một cây.
- Ctrl+C ở server đẩy sự kiện `stop` vào hàng đợi; vòng trò chơi in `Dừng server` rồi thoát.
- Kiểm thử: Linux, .NET SDK 10.0.114. Chỉ dùng BCL, không cần gói NuGet.

## Bài tập gợi ý
1. Thêm lệnh `chat {text}` để người chơi nhắn cho nhau trong ván; server kiểm tra độ dài và phát lại cho mọi người.
2. Đổi luật sang **đua tốc độ**: mọi người đoán cùng lúc, không theo lượt; mỗi người có khoảng riêng; ai đúng trước thắng. Máy trạng thái thay đổi thế nào?
3. Giới hạn tần suất: client gửi quá 5 thông điệp sai trong 10 giây thì bị ngắt kết nối (chống spam).
4. Cho phép người chơi kết nối lại (reconnect) trong 30 giây sau khi mất mạng mà vẫn giữ chỗ trong vòng lượt, dùng một mã phiên (token) server cấp trong `welcome`.
5. Viết bot thắng nhanh nhất trung bình khi chơi cùng bot `ngau_nhien`, đo bằng 100 ván tự động.

## Đọc thêm
- Bài 3.2 (định khung) và Bài 3.3 (giao thức tra cứu) của kho này; Mục 3.4 của giáo trình.
- RFC 9293 – *Transmission Control Protocol (TCP)*, IETF, 2022 (luồng byte, không giữ ranh giới thông điệp).
- RFC 8259 – *The JavaScript Object Notation (JSON) Data Interchange Format*, IETF, 2017.
- RFC 896 – John Nagle, *Congestion Control in IP/TCP Internetworks*, 1984 (thuật toán Nagle, lý do bật `TCP_NODELAY`).
- Tài liệu Python: [`socket`](https://docs.python.org/3/library/socket.html), [`queue`](https://docs.python.org/3/library/queue.html).

Mã nguồn của bài được viết mới cho kho này.
