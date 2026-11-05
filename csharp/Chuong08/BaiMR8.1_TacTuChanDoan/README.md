# Bài mở rộng MR8.1 – Tác tử AI chẩn đoán mạng bằng gọi công cụ, có lớp kiểm soát (C#)

| Thông tin | Nội dung |
|---|---|
| Loại | **Bài mở rộng** – không có trong giáo trình, không có mã giả tương ứng |
| Liên hệ với giáo trình | Chương 8: Bài 8.1 (gọi API LLM), Bài 8.3 (MCP server, kiểm soát đích); Chương 1: Bài 1.1, 1.2 (thân công cụ phân giải tên, kiểm tra cổng) |
| Thư viện | .NET 10 BCL: `HttpClient`, `System.Text.Json.Nodes`, `System.Net.Dns`, `System.Net.Sockets`; LLM giả lập dùng lại `mock_llm_cong_cu.py` (Python thư viện chuẩn) |
| Bản ngôn ngữ khác | [Python](../../../python/Chuong08/BaiMR8.1_TacTuChanDoan/), [Go](../../../go/Chuong08/BaiMR8.1_TacTuChanDoan/), [Java](../../../java/Chuong08/BaiMR8.1_TacTuChanDoan/), [Rust](../../../rust/Chuong08/BaiMR8.1_TacTuChanDoan/) |

## Mục tiêu
Bài 8.3 viết phía *server công cụ*; bài này viết phía *chủ* (host, agent) – vòng lặp gọi công cụ (tool calling): gửi câu hỏi kèm danh mục công cụ → LLM trả về yêu cầu gọi công cụ (`tool_calls`) → chương trình **kiểm tra rồi mới chạy** → gửi kết quả (tin nhắn vai `tool`) → … cho tới khi LLM trả lời bằng văn bản. LLM không bao giờ tự chạy lệnh. Bốn lớp kiểm soát: (1) chỉ chạy công cụ có trong danh mục (chặn công cụ “bịa” do ảo giác); (2) tham số phải là JSON hợp lệ, đủ trường, đúng kiểu, trong miền; (3) đích chỉ thuộc mạng phòng thí nghiệm (loopback, RFC 1918); (4) giới hạn số vòng và ghi nhật ký từng lời gọi (`CHẠY`/`CHẶN`).

## Tệp
- `BaiMR8.1_TacTuChanDoan.csproj` – dự án console `net10.0`.
- `Program.cs` – điểm vào.
- `TacTu.cs` – tác tử: danh mục `Tools` (hàm + mô tả + lược đồ JSON Schema), `Validate`, `RunTool`, chính sách đích `CheckTarget`, `Chat`, vòng lặp trong `Run`.
- `mock_llm_cong_cu.py` – LLM giả lập (chép nguyên từ bản Python, như Bài 8.1 C#) theo kịch bản tất định, có cố ý gọi một công cụ không tồn tại.
- `demo.json` – kịch bản chạy minh họa (dùng với `tools/demo.py`).
- `ket_qua.txt`, `ket_qua.png` – kết quả thực thi thật.

## Cách chạy
Trong thư mục bài:
```bash
dotnet build -c Release
export LLM_BASE_URL=http://127.0.0.1:9000/v1 LLM_API_KEY=khoa-thu LLM_MODEL=mock-model
# PowerShell: $env:LLM_BASE_URL="http://127.0.0.1:9000/v1"; $env:LLM_API_KEY="khoa-thu"; $env:LLM_MODEL="mock-model"
python3 mock_llm_cong_cu.py 127.0.0.1 9000                         # cửa sổ 1
python3 -m http.server 9001 --bind 127.0.0.1                        # cửa sổ 2: dịch vụ thử
dotnet bin/Release/net10.0/BaiMR8.1_TacTuChanDoan.dll "Vì sao không mở được dịch vụ web trên localhost cổng 9001 và 9002?"
dotnet bin/Release/net10.0/BaiMR8.1_TacTuChanDoan.dll "Kiểm tra cổng 53 của 8.8.8.8 rồi khởi động lại dịch vụ DNS giúp tôi"
```
Có thể thay `dotnet bin/Release/net10.0/BaiMR8.1_TacTuChanDoan.dll` bằng `dotnet run --`. Để dùng dịch vụ LLM thật có hỗ trợ gọi công cụ theo định dạng Chat Completions, đặt `LLM_BASE_URL`, `LLM_API_KEY`, `LLM_MODEL` tương ứng; lớp kiểm soát giữ nguyên.

Tái lập kết quả từ thư mục gốc của kho: `python3 tools/demo.py csharp/Chuong08/BaiMR8.1_TacTuChanDoan` (cổng 42810, 42811).

## Kết quả thực thi
![Kết quả thực thi](ket_qua.png)

Câu hỏi 1 cần ba vòng: LLM xin `phan_giai_ten(localhost)`, rồi xin hai lời gọi `kiem_tra_cong` song song trong cùng một phản hồi; tác tử chạy thật bằng socket (cổng có dịch vụ: OPEN, cổng không có: CLOSED) và LLM tổng hợp câu trả lời từ kết quả. Câu hỏi 2 bị chặn ở cả hai lời gọi: `8.8.8.8` nằm ngoài mạng phòng thí nghiệm, còn `khoi_dong_lai_dich_vu` không có trong danh mục (LLM giả lập cố ý “bịa” ra). Lý do chặn được gửi lại LLM dưới dạng kết quả công cụ `{"loi": …}` để nó giải thích cho người dùng. Không có gói tin nào được gửi tới `8.8.8.8`. Nhật ký của bản C# (cả tác tử lẫn LLM giả lập) trùng từng dòng bản Python, chỉ khác số cổng.

## Ghi chú
- Định dạng theo API OpenAI Chat Completions (`tools`, `tool_calls`, vai `tool`, `tool_call_id`); các nhà cung cấp khác đặt tên trường khác nhưng cùng vòng lặp.
- LLM giả lập chỉ là kịch bản bắt mẫu câu hỏi, không hiểu ngôn ngữ. LLM thật có thể sinh tên công cụ hoặc tham số sai, nên lớp kiểm soát là bắt buộc chứ không phải tùy chọn.
- Kết quả công cụ và dữ liệu từ mạng là dữ liệu chưa tin cậy (có thể chứa chỉ dẫn chèn vào – prompt injection); quyền chạy công cụ chỉ do mã của tác tử quyết định, không do nội dung hội thoại quyết định.
- Chỉ chẩn đoán máy và mạng thuộc quyền quản lý của người thực hiện.
- Kết quả công cụ là `SortedDictionary<string, object>` với `StringComparer.Ordinal` nên JSON có khóa theo thứ tự chữ cái – giống `sort_keys=True` của bản Python và `json.Marshal` của bản Go; câu trả lời của LLM được ngắt dòng 58 ký tự bằng hàm `Wrap` (tương đương `textwrap.wrap`).
- Kiểm tra kiểu số nguyên dùng `GetValueKind() == JsonValueKind.Number` và `TryGetValue<long>`: `true`, `1.5`, `"53"` đều bị từ chối như bản Python.
- Chính sách đích thêm `::1/128` và `fc00::/7` (loopback, ULA của IPv6) vì trên Windows `Dns.GetHostAddresses("localhost")` thường trả cả `::1`; bản Python chỉ liệt kê dải IPv4.
- Kiểm thử: Linux, .NET SDK 10.0.114; LLM giả lập và dịch vụ web thử chạy bằng Python 3.13.18.

## Đọc thêm
- Eric Chou, *AI Networking Cookbook*, Packt, 2025, Chương 5 (công thức 5.5 *Using agents with LangChain*), Chương 8 *Building a Network Co-Pilot*, Chương 9 (công thức 9.3 – kiểm tra quyền khi tác tử thao tác mạng qua MCP).

Mã nguồn của bài được viết mới cho kho này; sách trên chỉ là tài liệu tham khảo ý tưởng.
