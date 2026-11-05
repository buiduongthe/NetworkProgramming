# Bài mở rộng MR8.1 – Tác tử AI chẩn đoán mạng bằng gọi công cụ, có lớp kiểm soát (Rust)

| Thông tin | Nội dung |
|---|---|
| Loại | **Bài mở rộng** – không có trong giáo trình, không có mã giả tương ứng |
| Liên hệ với giáo trình | Chương 8: Bài 8.1 (gọi API LLM), Bài 8.3 (MCP server, kiểm soát đích); Chương 1: Bài 1.1, 1.2 (thân công cụ phân giải tên, kiểm tra cổng) |
| Thư viện | Thư viện chuẩn Rust: `std::net` (`ToSocketAddrs`, `TcpStream::connect_timeout`, `IpAddr`); crate `serde_json` 1.0.154, `rustls` 0.23.46 + `rustls-native-certs` 0.8.5 (khi `LLM_BASE_URL` là https://). Máy chủ LLM giả lập dùng lại tệp Python của bài |
| Bản ngôn ngữ khác | [C#](../../../csharp/Chuong08/BaiMR8.1_TacTuChanDoan/), [Java](../../../java/Chuong08/BaiMR8.1_TacTuChanDoan/), [Python](../../../python/Chuong08/BaiMR8.1_TacTuChanDoan/), [Go](../../../go/Chuong08/BaiMR8.1_TacTuChanDoan/) |

## Mục tiêu
Bài 8.3 viết phía *server công cụ*; bài này viết phía *chủ* (host, agent) – vòng lặp gọi công cụ (tool calling): gửi câu hỏi kèm danh mục công cụ → LLM trả về yêu cầu gọi công cụ (`tool_calls`) → chương trình **kiểm tra rồi mới chạy** → gửi kết quả (tin nhắn vai `tool`) → … cho tới khi LLM trả lời bằng văn bản. LLM không bao giờ tự chạy lệnh. Bốn lớp kiểm soát: (1) chỉ chạy công cụ có trong danh mục (chặn công cụ “bịa” do ảo giác); (2) tham số phải là JSON hợp lệ, đủ trường, đúng kiểu, trong miền; (3) đích chỉ thuộc mạng phòng thí nghiệm (loopback, RFC 1918); (4) giới hạn số vòng và ghi nhật ký từng lời gọi (`CHẠY`/`CHẶN`).

## Tệp
- `Cargo.toml`, `Cargo.lock` – gói Cargo `mr8_1_tac_tu_chan_doan`.
- `src/main.rs` – tác tử: danh mục `tools()` (hàm + lược đồ JSON Schema), `validate`, `run_tool`, `chat`, `wrap`, vòng lặp trong `main`; có `cargo test` cho các lớp kiểm soát.
- `src/http_post.rs` – POST JSON trên HTTP/1.1 viết tay (http:// hoặc https:// qua rustls), đọc thân theo `Content-Length`/chunked.
- `mock_llm_cong_cu.py` – chép nguyên từ bản Python: LLM giả lập theo kịch bản tất định, có cố ý gọi một công cụ không tồn tại.
- `demo.json` – kịch bản chạy minh họa (dùng với `tools/demo.py`).
- `ket_qua.txt`, `ket_qua.png` – kết quả thực thi thật.

## Cách chạy
Trong thư mục bài (`cargo build -q --release` một lần; `B=../../target/release/mr8_1_tac_tu_chan_doan`):
```bash
export LLM_BASE_URL=http://127.0.0.1:9000/v1 LLM_API_KEY=khoa-thu LLM_MODEL=mock-model
python3 mock_llm_cong_cu.py 127.0.0.1 9000                         # cửa sổ 1
python3 -m http.server 9001 --bind 127.0.0.1                        # cửa sổ 2: dịch vụ thử
$B "Vì sao không mở được dịch vụ web trên localhost cổng 9001 và 9002?"
$B "Kiểm tra cổng 53 của 8.8.8.8 rồi khởi động lại dịch vụ DNS giúp tôi"
```
Để dùng dịch vụ LLM thật có hỗ trợ gọi công cụ theo định dạng Chat Completions, đặt `LLM_BASE_URL` (https://…), `LLM_API_KEY`, `LLM_MODEL` tương ứng; lớp kiểm soát giữ nguyên. Trên Windows (PowerShell) đặt biến bằng `$env:LLM_API_KEY="..."`.

Tái lập kết quả từ thư mục gốc của kho: `python3 tools/demo.py rust/Chuong08/BaiMR8.1_TacTuChanDoan` (mock 44810, dịch vụ thử 44811).

## Kết quả thực thi
![Kết quả thực thi](ket_qua.png)

Câu hỏi 1 cần ba vòng: LLM xin `phan_giai_ten(localhost)`, rồi xin hai lời gọi `kiem_tra_cong` song song trong cùng một phản hồi; tác tử chạy thật bằng socket (cổng có dịch vụ: OPEN, cổng không có: CLOSED) và LLM tổng hợp câu trả lời từ kết quả. Câu hỏi 2 bị chặn ở cả hai lời gọi: `8.8.8.8` nằm ngoài mạng phòng thí nghiệm, còn `khoi_dong_lai_dich_vu` không có trong danh mục (LLM giả lập cố ý “bịa” ra). Lý do chặn được gửi lại LLM dưới dạng kết quả công cụ `{"loi": …}` để nó giải thích cho người dùng. Không có gói tin nào được gửi tới `8.8.8.8`. Bản Rust in kết quả giống hệt bản Python (trừ số cổng).

## Ghi chú
- Định dạng theo API OpenAI Chat Completions (`tools`, `tool_calls`, vai `tool`, `tool_call_id`); các nhà cung cấp khác đặt tên trường khác nhưng cùng vòng lặp.
- LLM giả lập chỉ là kịch bản bắt mẫu câu hỏi, không hiểu ngôn ngữ. LLM thật có thể sinh tên công cụ hoặc tham số sai, nên lớp kiểm soát là bắt buộc chứ không phải tùy chọn.
- Kết quả công cụ và dữ liệu từ mạng là dữ liệu chưa tin cậy (có thể chứa chỉ dẫn chèn vào – prompt injection); quyền chạy công cụ chỉ do mã của tác tử quyết định, không do nội dung hội thoại quyết định.
- Chỉ chẩn đoán máy và mạng thuộc quyền quản lý của người thực hiện.
- Kết quả công cụ được mã hóa JSON gọn, khóa theo thứ tự chữ cái (hàm `compact` sắp xếp khóa) để giống bản Python (`sort_keys=True`) và Go.
- Chính sách đích tự viết bằng phép AND với mặt nạ trên `u32` (thư viện chuẩn Rust không có kiểu mạng như `ipaddress`); địa chỉ IPv6 không thuộc danh sách cho phép nên bị chặn.
- Câu trả lời được ngắt dòng 58 ký tự bằng hàm `wrap` tự viết (tương đương `textwrap.wrap` với văn bản thường).
- Kiểm thử: Linux, Rust 1.97.0.

## Đọc thêm
- Eric Chou, *AI Networking Cookbook*, Packt, 2025, Chương 5 (công thức 5.5 *Using agents with LangChain*), Chương 8 *Building a Network Co-Pilot*, Chương 9 (công thức 9.3 – kiểm tra quyền khi tác tử thao tác mạng qua MCP).

Mã nguồn của bài được viết mới cho kho này; sách trên chỉ là tài liệu tham khảo ý tưởng.
