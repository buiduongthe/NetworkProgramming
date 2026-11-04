import java.io.ByteArrayOutputStream;
import java.io.IOException;
import java.io.OutputStream;
import java.net.InetAddress;
import java.net.InetSocketAddress;
import java.net.Socket;
import java.nio.charset.StandardCharsets;
import java.util.LinkedHashMap;
import java.util.Map;

/**
 * Bài 7.1 – Client HTTP/1.1 tối giản trên socket TCP (Mã giả 7.1, Mục 7.2.1 Giáo trình Lập trình mạng).
 * Kế thừa: Mã giả 2.2 (SAFE_CONNECT), 3.4 (RECV_EXACT), 3.5 (READ_LINE, Reader).
 * Biên dịch: javac -encoding UTF-8 -d out *.java
 * Chạy:      java -cp out MiniHttpClient [host] [port] [đường_dẫn ...]   (mặc định 127.0.0.1 8080 /)
 */
public class MiniHttpClient {
    record Response(int status, Map<String, String> hdr, byte[] data) {}

    public static void main(String[] args) throws IOException {
        String host = args.length >= 1 ? args[0] : "127.0.0.1";
        int port = args.length >= 2 ? Integer.parseInt(args[1]) : 8080;
        String[] paths = args.length >= 3 ? java.util.Arrays.copyOfRange(args, 2, args.length) : new String[] {"/"};
        for (String path : paths) {
            Response resp = httpRequest(host, port, "GET", path, Map.of("Accept", "application/json"), null);
            System.out.println("[client] GET " + path + " -> " + resp.status());
            String how = resp.hdr().getOrDefault("transfer-encoding", "").toLowerCase().endsWith("chunked")
                    ? "Transfer-Encoding = chunked"
                    : resp.hdr().containsKey("content-length") ? "Content-Length = " + resp.hdr().get("content-length")
                    : "đọc đến khi server đóng kết nối";
            System.out.println("[client] Định khung thân: " + how);
            System.out.println("[client] Thân (" + resp.data().length + " byte): "
                    + new String(resp.data(), StandardCharsets.UTF_8));
        }
    }

    /** Mã giả 7.1, dòng 1–23: gửi một yêu cầu HTTP/1.1 và đọc phản hồi. */
    static Response httpRequest(String host, int port, String method, String path,
                                Map<String, String> headers, byte[] body) throws IOException {
        try (Socket s = safeConnect(host, port, 3000)) {                    // try-with-resources = CLOSE
            s.setSoTimeout(5000);
            Reader r = new Reader(s.getInputStream());
            StringBuilder h = new StringBuilder(method + " " + path + " HTTP/1.1\r\n");
            h.append("Host: ").append(host).append(":").append(port).append("\r\n");   // kèm cổng
            headers.forEach((k, v) -> h.append(k).append(": ").append(v).append("\r\n"));
            if (body != null) h.append("Content-Length: ").append(body.length).append("\r\n");
            h.append("Connection: close\r\n\r\n");                         // dòng trống kết thúc tiêu đề
            OutputStream out = s.getOutputStream();
            out.write(h.toString().getBytes(StandardCharsets.US_ASCII));    // SEND_ALL: write() gửi hết
            if (body != null) out.write(body);
            out.flush();
            String line = r.readLine(8192);
            if (line == null) throw new IOException("Server đóng kết nối trước khi gửi phản hồi");
            int status = Integer.parseInt(line.split(" ")[1]);
            Map<String, String> hdr = readHeaders(r);
            String te = hdr.getOrDefault("transfer-encoding", "").toLowerCase();
            byte[] data;
            if (te.endsWith("chunked")) data = readChunked(r);             // vd. "gzip, chunked"
            else if (hdr.containsKey("content-length")) data = r.readExact(Integer.parseInt(hdr.get("content-length")));
            else data = r.readUntilClose();
            return new Response(status, hdr, data);
        }
    }

    /** Mã giả 7.1, dòng 25–34: đọc các trường tiêu đề đến dòng trống. */
    static Map<String, String> readHeaders(Reader r) throws IOException {
        Map<String, String> hdr = new LinkedHashMap<>();
        while (true) {
            String line = r.readLine(8192);
            if (line == null)                                               // dòng 29: đóng giữa phần tiêu đề
                throw new java.io.EOFException("Kết nối đóng khi chưa đọc hết phần tiêu đề");   // ConnectionClosed
            if (line.isEmpty()) return hdr;                                 // dòng 30: dòng trống – hết
            int i = line.indexOf(':');                                      // SPLIT_FIRST tại ':' đầu tiên
            String k = i >= 0 ? line.substring(0, i) : line, v = i >= 0 ? line.substring(i + 1) : "";
            hdr.put(k.trim().toLowerCase(), v.trim());                      // không phân biệt hoa/thường
        }
    }

    /** Mã giả 7.1, dòng 35–45: giải mã thân Transfer-Encoding: chunked. */
    static byte[] readChunked(Reader r) throws IOException {
        ByteArrayOutputStream data = new ByteArrayOutputStream();
        while (true) {
            String sizeLine = r.readLine(1024);
            if (sizeLine == null) throw new IOException("Thiếu dòng độ dài khối");
            int n = Integer.parseInt(sizeLine.split(";")[0].trim(), 16);   // bỏ phần mở rộng sau ';'
            if (n == 0) break;
            data.write(r.readExact(n));
            r.readLine(2);                                                  // CRLF sau mỗi khối
        }
        readHeaders(r);                                                     // bỏ phần trailer
        return data.toByteArray();
    }

    /** Mã giả 2.2 (rút gọn) – SAFE_CONNECT: thử lần lượt các địa chỉ, có thời gian chờ kết nối. */
    static Socket safeConnect(String host, int port, int timeoutMs) throws IOException {
        IOException last = null;
        for (InetAddress a : InetAddress.getAllByName(host)) {
            Socket s = new Socket();
            try {
                s.connect(new InetSocketAddress(a, port), timeoutMs);
                return s;
            } catch (IOException e) {
                last = e;
                s.close();
            }
        }
        throw new IOException("Không kết nối được " + host + ":" + port + ": " + (last != null ? last.getMessage() : ""));
    }
}
