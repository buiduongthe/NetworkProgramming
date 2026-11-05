import java.io.BufferedReader;
import java.io.IOException;
import java.io.InputStreamReader;
import java.io.OutputStream;
import java.net.InetSocketAddress;
import java.net.ServerSocket;
import java.net.Socket;
import java.nio.charset.StandardCharsets;
import java.security.MessageDigest;
import java.util.ArrayList;
import java.util.HashMap;
import java.util.HexFormat;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Locale;
import java.util.Map;
import java.util.concurrent.atomic.AtomicInteger;
import java.util.regex.Matcher;
import java.util.regex.Pattern;

/**
 * Bài mở rộng MR7.1 – Server HTTP/1.1 tối giản trên socket, phục vụ tệp có hỗ trợ tải từng phần (Range, RFC 9110).
 *
 * Phục vụ một tệp nhị phân sinh sẵn tại /tep.bin:
 *  - HEAD: trả Content-Length, Accept-Ranges: bytes, ETag, X-Checksum-SHA256.
 *  - GET có "Range: bytes=a-" hoặc "bytes=a-b": trả 206 Partial Content + Content-Range;
 *    dải không thỏa mãn được: 416 + "Content-Range: bytes * /<kích thước>".
 *  - If-Range: <ETag>: nếu ETag không khớp (tệp đã đổi) thì bỏ qua Range, trả 200 toàn bộ.
 *  - Giả lập sự cố mạng: --so-lan-cat N yêu cầu GET đầu tiên bị ngắt kết nối sau --cat-sau byte.
 * Server viết thẳng trên ServerSocket (mỗi kết nối một luồng ảo, mỗi kết nối một yêu cầu – "Connection: close")
 * để điều khiển được việc ngắt kết nối giữa chừng.
 *
 * Chạy: java -cp out ServerTep [host] [port] [--kich-thuoc B] [--cat-sau B] [--so-lan-cat N]
 */
public class ServerTep {
    static byte[] data;
    static String etag, sha;
    static int catSau = 100000;
    static final AtomicInteger counter = new AtomicInteger(), cutsLeft = new AtomicInteger();
    static final Pattern RANGE = Pattern.compile("bytes=(\\d+)-(\\d*)");
    static final Map<Integer, String> REASON = Map.of(200, "OK", 206, "Partial Content", 404, "Not Found",
            416, "Requested Range Not Satisfiable", 405, "Method Not Allowed");

    /** Nội dung tệp cố định (giống bản Python, Go) để các bản cho cùng SHA-256. */
    static byte[] makeData(int size) {
        byte[] b = new byte[size];
        for (int i = 0; i < size; i++) b[i] = (byte) ((i * 31L + i / 7) % 256);
        return b;
    }

    static void log(String s) { System.out.println("[server] " + s); }

    /** Gửi dòng trạng thái và các tiêu đề chung. */
    static void headers(OutputStream out, int status, long length, Map<String, String> extra) throws IOException {
        StringBuilder b = new StringBuilder("HTTP/1.1 " + status + " " + REASON.get(status) + "\r\n");
        Map<String, String> h = new LinkedHashMap<>();
        h.put("Content-Type", "application/octet-stream");
        h.put("Content-Length", String.valueOf(length));
        h.put("Accept-Ranges", "bytes");
        h.put("ETag", etag);
        h.put("Connection", "close");
        h.putAll(extra);
        for (Map.Entry<String, String> e : h.entrySet()) b.append(e.getKey()).append(": ").append(e.getValue()).append("\r\n");
        out.write(b.append("\r\n").toString().getBytes(StandardCharsets.ISO_8859_1));
    }

    static void handle(Socket s) {
        try (s) {
            BufferedReader in = new BufferedReader(new InputStreamReader(s.getInputStream(), StandardCharsets.ISO_8859_1));
            String line = in.readLine();                            // dòng yêu cầu: "GET /tep.bin HTTP/1.1"
            if (line == null) return;
            String[] req = line.split(" ");
            Map<String, String> hdr = new HashMap<>();
            String h;
            while ((h = in.readLine()) != null && !h.isEmpty()) {   // tiêu đề tới dòng trống
                int i = h.indexOf(':');
                if (i > 0) hdr.put(h.substring(0, i).strip().toLowerCase(Locale.ROOT), h.substring(i + 1).strip());
            }
            int k = counter.incrementAndGet();
            OutputStream out = s.getOutputStream();
            if (req.length < 2 || !req[1].equals("/tep.bin")) {
                headers(out, 404, 0, Map.of());
                return;
            }
            int size = data.length;
            if (req[0].equals("HEAD")) {
                headers(out, 200, size, Map.of("X-Checksum-SHA256", sha));
                log("#" + k + " HEAD -> 200, " + size + " byte");
                return;
            }
            if (!req[0].equals("GET")) {
                headers(out, 405, 0, Map.of());
                return;
            }
            String rng = hdr.get("range"), ifRange = hdr.get("if-range");
            int start = 0, end = size - 1, status = 200;
            if (rng != null && (ifRange == null || ifRange.equals(etag))) {   // If-Range khớp mới dùng Range
                Matcher m = RANGE.matcher(rng.strip());
                if (m.matches()) {
                    long st = Long.parseLong(m.group(1));
                    long en = m.group(2).isEmpty() ? size - 1 : Math.min(Long.parseLong(m.group(2)), size - 1);
                    if (st >= size || st > en) {
                        headers(out, 416, 0, Map.of("Content-Range", "bytes */" + size));
                        log("#" + k + " GET Range=" + rng + " -> 416");
                        return;
                    }
                    start = (int) st;
                    end = (int) en;
                    status = 206;
                }
            }
            int len = end - start + 1;
            headers(out, status, len, status == 206 ? Map.of("Content-Range", "bytes " + start + "-" + end + "/" + size) : Map.of());
            String r = rng == null ? "-" : rng;
            if (cutsLeft.getAndUpdate(x -> Math.max(0, x - 1)) > 0) {    // giả lập đứt kết nối giữa chừng
                out.write(data, start, Math.min(catSau, len));
                out.flush();
                s.shutdownOutput();
                log("#" + k + " GET Range=" + r + " -> " + status + ", NGẮT sau " + Math.min(catSau, len) + " byte");
                return;
            }
            out.write(data, start, len);
            log("#" + k + " GET Range=" + r + " -> " + status + ", gửi " + len + " byte");
        } catch (IOException ignored) { }
    }

    public static void main(String[] args) throws Exception {
        List<String> pos = new ArrayList<>();
        int kichThuoc = 300000, soLanCat = 0;
        for (int i = 0; i < args.length; i++) {
            switch (args[i]) {
                case "--kich-thuoc" -> kichThuoc = Integer.parseInt(args[++i]);
                case "--cat-sau" -> catSau = Integer.parseInt(args[++i]);
                case "--so-lan-cat" -> soLanCat = Integer.parseInt(args[++i]);
                default -> pos.add(args[i]);
            }
        }
        String host = pos.size() > 0 ? pos.get(0) : "127.0.0.1";
        int port = pos.size() > 1 ? Integer.parseInt(pos.get(1)) : 8000;
        data = makeData(kichThuoc);
        sha = HexFormat.of().formatHex(MessageDigest.getInstance("SHA-256").digest(data));
        etag = "\"" + sha.substring(0, 16) + "\"";
        cutsLeft.set(soLanCat);
        ServerSocket srv = new ServerSocket();
        srv.setReuseAddress(true);
        srv.bind(new InetSocketAddress(host, port), 16);
        log("http://" + host + ":" + port + "/tep.bin, " + data.length + " byte, ETag " + etag);
        Runtime.getRuntime().addShutdownHook(new Thread(() -> log("Dừng server")));
        while (true) {
            Socket s = srv.accept();
            Thread.ofVirtual().start(() -> handle(s));
        }
    }
}
