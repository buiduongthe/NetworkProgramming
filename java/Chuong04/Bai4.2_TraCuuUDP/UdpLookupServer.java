import java.io.IOException;
import java.net.DatagramPacket;
import java.net.DatagramSocket;
import java.net.InetSocketAddress;
import java.net.PortUnreachableException;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.LinkedHashMap;
import java.util.Map;

/**
 * Bài 4.2 – Server tra cứu thuật ngữ trên UDP (Mã giả 4.3, Mục 4.3.1 Giáo trình Lập trình mạng).
 * Kế thừa: Mã giả 4.1 (vòng RECVFROM/SENDTO), server tra cứu TCP (PROCESS_REQUEST, DICT).
 * Chạy (trong thư mục bài, nơi có glossary.json):
 *   java -cp out UdpLookupServer [host] [port] [so_datagram_bo_qua]   (mặc định 0.0.0.0 9100 0)
 */
public class UdpLookupServer {
    static final int MAX_DGRAM = 1200;                                                   // dòng 1
    static Map<String, Object> DICT;

    /** PROCESS_REQUEST: nghiệp vụ, độc lập với socket (dùng lại nguyên vẹn từ bản TCP). */
    static Map<String, Object> processRequest(Map<String, Object> req) {
        Map<String, Object> r = new LinkedHashMap<>();
        Object op = req.get("op");
        if ("lookup".equals(op)) {
            Object term = req.get("term");
            if (term instanceof String t && DICT.containsKey(t)) {
                r.put("status", 200); r.put("term", t); r.put("text", DICT.get(t));
            } else {
                r.put("status", 404); r.put("error", "Không có thuật ngữ");
            }
        } else if ("list".equals(op)) {
            r.put("status", 200); r.put("terms", new ArrayList<>(DICT.keySet()));
        } else {
            r.put("status", 400); r.put("error", "Sai thao tác");
        }
        return r;
    }

    static byte[] toBytes(Map<String, Object> obj) { return Json.stringify(obj).getBytes(StandardCharsets.UTF_8); }

    @SuppressWarnings("unchecked")
    public static void main(String[] args) throws IOException {
        DICT = (Map<String, Object>) Json.parse(Files.readString(Path.of("glossary.json")));  // LOAD_JSON_FILE
        String[] ep2 = java.util.Arrays.copyOf(args, Math.min(args.length, 2));
        Object[] ep = NetUtil.parseEndpoint(ep2, "0.0.0.0", 9100);                         // dòng 3
        String host = (String) ep[0];
        int port = (Integer) ep[1];
        int drop = args.length > 2 ? Integer.parseInt(args[2]) : 0;                        // mô phỏng mất gói (thêm)
        try (DatagramSocket s = new DatagramSocket(new InetSocketAddress(host, port))) {   // dòng 4–5
            System.out.println("[server] Đang lắng nghe UDP tại " + host + ":" + port + ", từ điển " + DICT.size() + " thuật ngữ");
            if (drop > 0) System.out.println("[server] Mô phỏng mất gói: bỏ qua " + drop + " datagram đầu tiên");
            Runtime.getRuntime().addShutdownHook(new Thread(() -> System.out.println("[server] Dừng server")));
            byte[] buf = new byte[65535];
            int count = 0;
            while (true) {                                                                 // dòng 6: LOOP
                DatagramPacket p = new DatagramPacket(buf, buf.length);
                try { s.receive(p); } catch (PortUnreachableException e) { continue; }     // dòng 7
                String addr = p.getAddress().getHostAddress() + ":" + p.getPort();
                if (++count <= drop) {
                    System.out.println("[server] Bỏ qua datagram thứ " + count + " từ " + addr + " (mô phỏng mất gói)");
                    continue;
                }
                Map<String, Object> req, resp;
                try {                                                                      // dòng 8–14
                    Object o = Json.parse(new String(buf, 0, p.getLength(), StandardCharsets.UTF_8));
                    if (!(o instanceof Map)) throw new IllegalArgumentException("không phải đối tượng JSON");
                    req = (Map<String, Object>) o;
                    resp = processRequest(req);
                } catch (IllegalArgumentException e) {                                     // CATCH JsonError
                    req = new LinkedHashMap<>(); req.put("id", null);
                    resp = new LinkedHashMap<>(); resp.put("status", 400); resp.put("error", "JSON sai");
                }
                resp.put("id", req.get("id"));                                             // dòng 15
                byte[] out = toBytes(resp);                                                // dòng 16
                if (out.length > MAX_DGRAM) {                                              // dòng 17–20
                    resp = new LinkedHashMap<>();
                    resp.put("id", req.get("id")); resp.put("status", 413); resp.put("error", "Quá lớn");
                    out = toBytes(resp);
                }
                s.send(new DatagramPacket(out, out.length, p.getSocketAddress()));         // dòng 21
                System.out.println("[server] Yêu cầu id=" + Json.stringify(req.get("id")) + " op=" + req.get("op")
                        + " từ " + addr + " -> status=" + resp.get("status") + ", " + out.length + " byte");
            }
        }
    }
}
