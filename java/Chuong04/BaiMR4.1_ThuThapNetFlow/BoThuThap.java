import java.net.DatagramPacket;
import java.net.DatagramSocket;
import java.net.InetAddress;
import java.net.InetSocketAddress;
import java.net.SocketTimeoutException;
import java.nio.ByteBuffer;
import java.util.ArrayList;
import java.util.Arrays;
import java.util.HashMap;
import java.util.List;
import java.util.Locale;
import java.util.Map;

/**
 * Bài mở rộng MR4.1 – Bộ thu thập NetFlow v5 (collector) trên UDP.
 *
 * Bài mở rộng, không có trong giáo trình; vận dụng Chương 4: server UDP nhận datagram,
 * mỗi datagram là trọn một gói xuất (UDP giữ ranh giới thông điệp), giải mã nhị phân
 * bằng ByteBuffer như Bài 4.3 (truy vấn DNS). Bộ thu:
 *  - kiểm tra tính hợp lệ (phiên bản, độ dài = 24 + 48 x count) và loại gói hỏng;
 *  - phát hiện mất gói nhờ trường flow_sequence (giống số thứ tự ở Bài 4.4);
 *  - tổng hợp: nguồn dùng nhiều băng thông nhất, dịch vụ (cổng/giao thức) phổ biến.
 * Dừng khi nhàn rỗi quá --nhan-roi giây sau gói đầu tiên (hoặc Ctrl+C) và in báo cáo.
 *
 * Chạy: java -cp out BoThuThap [host] [port] [--nhan-roi S]
 */
public class BoThuThap {
    record Head(int count, long seq, int engine) {}
    record Rec(String src, String dst, long pkts, long bytes, int sport, int dport, int proto) {}
    record Parsed(Head head, List<Rec> records) {}

    static String ip(ByteBuffer b, int off) throws Exception {
        byte[] a = new byte[4];
        b.get(off, a);
        return InetAddress.getByAddress(a).getHostAddress();
    }

    /** Trả về (tiêu_đề, danh_sách_bản_ghi) hoặc ném IllegalArgumentException nếu gói không hợp lệ. */
    static Parsed parsePacket(byte[] data, int len) throws Exception {
        if (len < NetFlowV5.HEADER_SIZE) throw new IllegalArgumentException("quá ngắn (" + len + " byte)");
        ByteBuffer b = ByteBuffer.wrap(data, 0, len);                    // big-endian
        int version = b.getShort(0) & 0xFFFF, count = b.getShort(2) & 0xFFFF;
        long seq = b.getInt(16) & 0xFFFFFFFFL;
        int eid = b.get(21) & 0xFF;
        if (version != 5) throw new IllegalArgumentException("phiên bản " + version + ", không phải 5");
        if (count < 1 || count > NetFlowV5.MAX_RECORDS || len != NetFlowV5.HEADER_SIZE + count * NetFlowV5.RECORD_SIZE)
            throw new IllegalArgumentException("độ dài " + len + " không khớp count=" + count);
        List<Rec> records = new ArrayList<>();
        for (int i = 0; i < count; i++) {
            int o = NetFlowV5.HEADER_SIZE + i * NetFlowV5.RECORD_SIZE;
            records.add(new Rec(ip(b, o + NetFlowV5.R_SRC), ip(b, o + NetFlowV5.R_DST),
                    b.getInt(o + NetFlowV5.R_PKTS) & 0xFFFFFFFFL, b.getInt(o + NetFlowV5.R_OCTETS) & 0xFFFFFFFFL,
                    b.getShort(o + NetFlowV5.R_SPORT) & 0xFFFF, b.getShort(o + NetFlowV5.R_DPORT) & 0xFFFF,
                    b.get(o + NetFlowV5.R_PROTO) & 0xFF));
        }
        return new Parsed(new Head(count, seq, eid), records);
    }

    static volatile boolean reported = false;

    public static void main(String[] args) throws Exception {
        List<String> pos = new ArrayList<>();
        double idle = 1.5;
        for (int i = 0; i < args.length; i++) {
            if (args[i].equals("--nhan-roi")) idle = Double.parseDouble(args[++i]);
            else pos.add(args[i]);
        }
        String host = pos.size() > 0 ? pos.get(0) : "127.0.0.1";
        int port = pos.size() > 1 ? Integer.parseInt(pos.get(1)) : 2055;
        DatagramSocket sock = new DatagramSocket(new InetSocketAddress(host, port));
        System.out.println("[thu-thap] Nghe NetFlow v5 tại UDP " + host + ":" + port);
        Map<String, Long> bySrc = new HashMap<>();
        Map<List<Integer>, long[]> byService = new HashMap<>();      // (cổng, giao thức) -> {số luồng, byte}
        Map<String, Long> expected = new HashMap<>();                 // "địa chỉ máy xuất|engine" -> seq mong đợi
        long[] st = new long[5];                                      // ok, bad, flows, lost, totalBytes
        // Ctrl+C: vẫn in báo cáo như khối finally của bản Python
        Thread hook = new Thread(() -> report(st, bySrc, byService));
        Runtime.getRuntime().addShutdownHook(hook);
        byte[] buf = new byte[2048];                                  // gói v5 lớn nhất 24 + 30*48 = 1464 byte
        while (true) {
            DatagramPacket pk = new DatagramPacket(buf, buf.length);
            try {
                sock.receive(pk);                                     // RECVFROM
            } catch (SocketTimeoutException e) {
                System.out.println("[thu-thap] Nhàn rỗi " + idle + " s, kết thúc");
                break;
            }
            sock.setSoTimeout((int) (idle * 1000));                   // sau gói đầu tiên mới bắt đầu đếm nhàn rỗi
            String from = pk.getAddress().getHostAddress();
            Parsed p;
            try {
                p = parsePacket(pk.getData(), pk.getLength());
            } catch (IllegalArgumentException e) {
                st[1]++;
                System.out.println("[thu-thap] Loại gói từ " + from + ":" + pk.getPort() + ": " + e.getMessage());
                continue;
            }
            Head h = p.head();
            String key = from + "|" + h.engine();
            Long exp = expected.get(key);
            if (exp != null && h.seq() != exp) {
                long gap = h.seq() - exp;
                st[3] += Math.max(gap, 0);
                System.out.println("[thu-thap] MẤT GÓI: chờ seq=" + exp + ", nhận seq=" + h.seq() + ", thiếu " + gap + " luồng");
            }
            expected.put(key, h.seq() + h.count());
            st[0]++;
            st[2] += h.count();
            System.out.println("[thu-thap] Gói seq=" + h.seq() + ": " + h.count() + " luồng");
            for (Rec r : p.records()) {
                bySrc.merge(r.src(), r.bytes(), Long::sum);
                long[] svc = byService.computeIfAbsent(List.of(r.dport(), r.proto()), k -> new long[2]);
                svc[0]++;
                svc[1] += r.bytes();
                st[4] += r.bytes();
            }
        }
        sock.close();
        Runtime.getRuntime().removeShutdownHook(hook);
        report(st, bySrc, byService);
    }

    static synchronized void report(long[] st, Map<String, Long> bySrc, Map<List<Integer>, long[]> byService) {
        if (reported) return;
        reported = true;
        long total = st[4];
        System.out.println("[thu-thap] Hợp lệ " + st[0] + " gói, loại " + st[1] + " gói, mất " + st[3] + " luồng");
        System.out.println("[thu-thap] Đã thu " + st[2] + " luồng, tổng " + total + " byte");
        if (total == 0) return;
        System.out.println("[thu-thap] Nguồn dùng nhiều byte nhất:");
        List<Map.Entry<String, Long>> srcs = new ArrayList<>(bySrc.entrySet());
        srcs.sort((a, b) -> !a.getValue().equals(b.getValue()) ? Long.compare(b.getValue(), a.getValue()) : a.getKey().compareTo(b.getKey()));
        for (Map.Entry<String, Long> e : srcs.subList(0, Math.min(3, srcs.size())))
            System.out.println(String.format(Locale.ROOT, "  %-12s %9d byte (%.1f%%)", e.getKey(), e.getValue(), e.getValue() * 100.0 / total));
        System.out.println("[thu-thap] Dịch vụ theo số luồng:");
        List<Map.Entry<List<Integer>, long[]>> svcs = new ArrayList<>(byService.entrySet());
        svcs.sort((a, b) -> {
            if (a.getValue()[0] != b.getValue()[0]) return Long.compare(b.getValue()[0], a.getValue()[0]);
            int c = Integer.compare(a.getKey().get(0), b.getKey().get(0));
            return c != 0 ? c : Integer.compare(a.getKey().get(1), b.getKey().get(1));
        });
        for (Map.Entry<List<Integer>, long[]> e : svcs) {
            int proto = e.getKey().get(1);
            String name = e.getKey().get(0) + "/" + NetFlowV5.PROTO_NAME.getOrDefault(proto, String.valueOf(proto));
            System.out.printf("  %-9s %3d luồng %9d byte%n", name, e.getValue()[0], e.getValue()[1]);
        }
    }
}
