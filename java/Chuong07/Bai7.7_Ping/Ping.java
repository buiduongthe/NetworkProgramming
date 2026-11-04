import java.io.IOException;
import java.net.Inet4Address;
import java.net.InetAddress;
import java.nio.ByteBuffer;
import java.security.SecureRandom;
import java.util.Arrays;
import java.util.HexFormat;

/**
 * Bài 7.7 – Kiểm tra máy đích có trả lời (Mã giả 7.8, Mục 7.5.3 Giáo trình Lập trình mạng).
 * Thư viện chuẩn Java KHÔNG có socket RAW, nên không tự gửi được gói ICMP như mã giả. Chương trình:
 *  (1) tạo gói Echo Request và tính INET_CHECKSUM (RFC 1071) đúng như mã giả, để minh họa cấu trúc gói;
 *  (2) đo thời gian khứ hồi bằng InetAddress.isReachable(): JDK gửi ICMP Echo nếu tiến trình có quyền
 *      dùng socket RAW, ngược lại thử kết nối TCP tới cổng 7 (echo); không lấy được TTL.
 * Biên dịch: javac -encoding UTF-8 -d out Ping.java
 * Chạy:      java -cp out Ping [host] [số_lần] [thời_gian_chờ_ms]   (mặc định 127.0.0.1 4 1000)
 */
public class Ping {
    /** Mã giả 7.8, dòng 1–11: tổng kiểm tra Internet (RFC 1071). */
    static int inetChecksum(byte[] b) {
        if (b.length % 2 == 1) b = Arrays.copyOf(b, b.length + 1);
        long sum = 0;
        for (int i = 0; i < b.length; i += 2) sum += ((b[i] & 0xFF) << 8) | (b[i + 1] & 0xFF);
        while (sum > 0xFFFF) sum = (sum & 0xFFFF) + (sum >> 16);      // cộng phần tràn
        return (int) (~sum & 0xFFFF);
    }

    /** Mã giả 7.8, dòng 17–20: gói Echo Request = kiểu 8, mã 0, checksum, id, seq, 32 byte dữ liệu. */
    static byte[] echoRequest(int id, int seq) {
        byte[] data = new byte[32];
        new SecureRandom().nextBytes(data);
        ByteBuffer p = ByteBuffer.allocate(8 + 32);                     // big-endian mặc định
        p.put((byte) 8).put((byte) 0).putShort((short) 0).putShort((short) id).putShort((short) seq).put(data);
        byte[] pkt = p.array();
        int c = inetChecksum(pkt);
        pkt[2] = (byte) (c >> 8); pkt[3] = (byte) c;                     // điền tổng kiểm tra
        return pkt;
    }

    public static void main(String[] args) throws IOException, InterruptedException {
        String host = args.length >= 1 ? args[0] : "127.0.0.1";
        int count = args.length >= 2 ? Integer.parseInt(args[1]) : 4;
        int timeoutMs = args.length >= 3 ? Integer.parseInt(args[2]) : 1000;
        InetAddress dst = Arrays.stream(InetAddress.getAllByName(host))   // FIRST_IPV4(RESOLVE(host))
                .filter(a -> a instanceof Inet4Address).findFirst().orElseThrow();
        byte[] pkt = echoRequest((int) (ProcessHandle.current().pid() & 0xFFFF), 1);
        System.out.println("[ping] Tiêu đề ICMP tự tạo: " + HexFormat.ofDelimiter(" ").withUpperCase().formatHex(pkt, 0, 8)
                + " (kiểu, mã, checksum, id, seq)");
        System.out.printf("[ping] Kiểm tra: INET_CHECKSUM(gói đã điền checksum) = 0x%04X (đúng khi bằng 0)%n", inetChecksum(pkt));
        System.out.println("[ping] Java không có socket RAW: đo bằng InetAddress.isReachable(), không có TTL");
        System.out.println("[ping] PING " + host + " (" + dst.getHostAddress() + "): " + count + " lần, chờ tối đa " + timeoutMs + " ms");
        int ok = 0; double total = 0;
        for (int seq = 1; seq <= count; seq++) {
            long t0 = System.nanoTime();                                // NOW_PRECISE()
            boolean reachable = dst.isReachable(timeoutMs);
            double rtt = (System.nanoTime() - t0) / 1e6;
            if (reachable) {
                ok++; total += rtt;
                System.out.printf("[ping] Trả lời từ %s: seq=%d thời gian=%.3f ms%n", dst.getHostAddress(), seq, rtt);
            } else {
                System.out.println("[ping] seq=" + seq + ": hết thời gian chờ");
            }
            if (seq < count) Thread.sleep(1000);
        }
        String avg = ok > 0 ? String.format("%.3f ms", total / ok) : "không có";
        System.out.println("[ping] Đã gửi " + count + ", nhận " + ok + ", mất " + 100 * (count - ok) / count + "%; RTT trung bình " + avg);
    }
}
