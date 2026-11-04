# Bài 7.5 – Dịch vụ gRPC tra cứu thuật ngữ (Java)

| Thông tin | Nội dung |
|---|---|
| Mã giả tương ứng | Mã giả 7.6 (Cài đặt và gọi dịch vụ gRPC – mã nghiệp vụ) |
| Mục giáo trình | 7.4.2 Protocol Buffers, 7.4.3 Bốn kiểu gọi và cơ chế truyền trên HTTP/2 |
| Kế thừa | Mã giả 3.7 (nhánh "lookup" của PROCESS_REQUEST thành phương thức Lookup) |
| Thư viện | NGOẠI LỆ thư viện (Maven): `grpc-netty-shaded`, `grpc-protobuf`, `grpc-stub` 1.68.1, protoc 3.25.5 qua `protobuf-maven-plugin` 0.6.1 |

## Mục tiêu
Cài đặt phần nghiệp vụ của dịch vụ gRPC theo hợp đồng `tra_cuu.proto` (Mục 7.4.2): lớp `GlossaryService` kế thừa lớp cơ sở do công cụ sinh, phương thức unary `Lookup` (báo `NOT_FOUND` bằng mã trạng thái gRPC) và phương thức luồng từ server `Watch` (lọc theo tiền tố, dừng khi client hủy); phía client gọi `Lookup` có hạn chót và đọc luồng `Watch`. Client in cách mã hóa Protocol Buffers của `LookupRequest{term: "TCP"}` để đối chiếu với ví dụ 5 byte `0A 03 54 43 50` của giáo trình. Bài tập “Dịch vụ gRPC có hạn chót” được minh họa bằng server cố ý chậm 2 giây và client đặt hạn chót 1 giây.

## Tệp
- `pom.xml` – dự án Maven: phụ thuộc grpc-java, `os-maven-plugin`, `protobuf-maven-plugin` (sinh mã từ `.proto`), `exec-maven-plugin`.
- `src/main/proto/tra_cuu.proto` – hợp đồng dịch vụ (dùng chung ba ngôn ngữ).
- `src/main/java/GlossaryServer.java` – `GlossaryService extends GlossaryGrpc.GlossaryImplBase` (`lookup`, `watch`) và máy chủ.
- `src/main/java/GlossaryClient.java` – stub chặn `GlossaryBlockingStub`: `lookup` có `withDeadlineAfter`, `watch` trả `Iterator`.

## Cách chạy
Cần Maven và mạng tới Maven Central ở lần build đầu (tải thư viện và bản protoc phù hợp hệ điều hành):
```bash
mvn -q compile
```
```bash
# Cửa sổ 1 – server
mvn -q exec:java -Dexec.mainClass=GlossaryServer -Dexec.args="0.0.0.0 5001"
# Cửa sổ 2 – client (hạn chót mặc định 2000 ms)
mvn -q exec:java -Dexec.mainClass=GlossaryClient -Dexec.args="127.0.0.1 5001"
# Bài tập hạn chót: server chậm 2 giây, client chờ 1 giây
mvn -q exec:java -Dexec.mainClass=GlossaryServer -Dexec.args="0.0.0.0 5002 2000"
mvn -q exec:java -Dexec.mainClass=GlossaryClient -Dexec.args="127.0.0.1 5002 1000"
```


## Ghi chú
- Trạng thái kiểm thử: **chưa chạy trong môi trường biên soạn** – môi trường không truy cập được Maven Central để tải grpc-java và protoc; chưa có ảnh kết quả. Mã viết theo cách dùng chuẩn của grpc-java; kết quả mong đợi giống bản Python (cùng thông điệp in ra).
- NGOẠI LỆ về cấu trúc: bài dùng bố cục Maven chuẩn (`src/main/java`, `src/main/proto`) thay cho `javac` trên thư mục bài; lớp nghiệp vụ vẫn ở gói mặc định, lớp sinh ra ở gói `glossary.v1`.
- Mã giả dùng gọi bất đồng bộ; bản Java dùng stub chặn cho gọn (grpc-java cũng có `GlossaryStub` bất đồng bộ và `GlossaryFutureStub`). Server chờ trên luồng của grpc-java và kiểm tra `ServerCallStreamObserver.isCancelled()` thay cho `ctx.cancelled`.
- `Grpc.newServerBuilderForPort` lắng nghe mọi địa chỉ; tham số `host` của server chỉ dùng để in.
- `RAISE RpcError(NOT_FOUND, …)` là `obs.onError(Status.NOT_FOUND.withDescription(…).asRuntimeException())`.
- Tệp `tra_cuu.proto` giống nhau ở ba ngôn ngữ; các dòng `option csharp_namespace`, `java_package`, `java_multiple_files` chỉ tác động tới trình sinh mã tương ứng. `Import` và `Sync` có trong hợp đồng nhưng không được cài đặt (gọi sẽ nhận `UNIMPLEMENTED`), đúng phạm vi của Mã giả 7.6.
- Mã giả dùng `https://glossary.local:5001` (HTTP/2 trên TLS); bài chạy HTTP/2 không mã hóa (h2c) trên máy cục bộ cho phòng thí nghiệm. Triển khai thật phải dùng TLS (Chương 8).
- `CHANGES_FEED()` được mô phỏng bằng danh sách cố định HTTP/2, QUIC, HTTP/3, gRPC, HTTPS, mỗi 0,5 giây một mục; `Watch(prefix='HTTP')` nhận ba mục.
- Tham số thứ ba của server là độ trễ cố ý của `Lookup` (ms), dùng cho bài tập hạn chót; mặc định 0.
- Kiểm thử: Linux, OpenJDK 21.0.12.1.
