import java.io.FileOutputStream;
import java.io.IOException;
import java.io.OutputStream;
import java.net.DatagramPacket;
import java.net.DatagramSocket;
import java.net.InetSocketAddress;
import java.net.PortUnreachableException;
import java.net.SocketTimeoutException;
import java.nio.ByteBuffer;

/**
 * Bài 4.4 – Bên nhận dừng và chờ (Mã giả 4.7 SAW_RECEIVE, Mục 4.4.1); kế thừa vòng RECVFROM/SENDTO của Mã giả 4.1.
 * Chạy: java -cp out SawRecv [host] [port] [tep_ra] [mat_goi]   (mặc định 0.0.0.0 9000 tep_nhan.bin)
 */
public class SawRecv {
    /** Mã giả 4.7 – SAW_RECEIVE; trả về {số byte đã ghi, số datagram trùng}. */
    static long[] sawReceive(DatagramSocket s, OutputStream out, SawCommon.LossSim loss) throws IOException {
        int expected = 0; long written = 0, dups = 0;                                // dòng 2
        byte[] d = new byte[65535];
        while (true) {                                                               // dòng 3: LOOP
            DatagramPacket p = new DatagramPacket(d, d.length);
            try {
                s.receive(p);                                                        // dòng 5
            } catch (SocketTimeoutException e) {                                     // dòng 6: chỉ xảy ra sau FIN
                System.out.println("[nhận] Hết " + 2 * SawCommon.MAX_RTO + " ms không có datagram mới: kết thúc");
                return new long[] {written, dups};                                   // dòng 7
            } catch (PortUnreachableException e) {
                continue;
            }
            int n = p.getLength();
            if (n < 5) continue;                                                     // dòng 9: datagram lỗi
            int seq = ByteBuffer.wrap(d, 0, 4).getInt();                             // dòng 10
            byte type = d[4];
            byte[] ack = ByteBuffer.allocate(5).putInt(seq).put(SawCommon.ACK).array();
            boolean dropped = loss.drop();
            if (!dropped) s.send(new DatagramPacket(ack, 5, p.getSocketAddress()));  // dòng 11: luôn ACK
            String note = dropped ? " – mô phỏng mất ACK" : "";
            if (seq == expected && type == SawCommon.FIN) {                          // dòng 12
                s.setSoTimeout(2 * SawCommon.MAX_RTO);                               // dòng 13: như TIME-WAIT
                System.out.println("[nhận] seq=" + seq + " FIN: gửi ACK " + seq + note + "; chờ "
                        + 2 * SawCommon.MAX_RTO + " ms (như TIME-WAIT)");
            } else if (seq == expected) {                                            // dòng 14
                out.write(d, 5, n - 5);                                              // dòng 15
                written += n - 5;
                expected++;                                                          // dòng 16
                System.out.println("[nhận] seq=" + seq + " DATA " + (n - 5) + " byte: ghi tệp, gửi ACK " + seq + note);
            } else {                                                                 // seq < expected: trùng
                dups++;
                System.out.println("[nhận] seq=" + seq + " trùng: bỏ dữ liệu, vẫn gửi ACK " + seq + note);
            }
        }
    }

    public static void main(String[] args) throws IOException {
        Object[] ep = SawCommon.parseEndpoint(java.util.Arrays.copyOf(args, Math.min(args.length, 2)), "0.0.0.0", 9000);
        String host = (String) ep[0];
        int port = (Integer) ep[1];
        String path = args.length > 2 ? args[2] : "tep_nhan.bin";
        SawCommon.LossSim loss = new SawCommon.LossSim(args.length > 3 ? args[3] : "");
        long[] r;
        try (DatagramSocket s = new DatagramSocket(new InetSocketAddress(host, port));
             OutputStream out = new FileOutputStream(path)) {
            System.out.println("[nhận] Đang chờ dữ liệu UDP tại " + host + ":" + port + ", ghi vào " + path);
            System.out.println("[nhận] Mô phỏng mất ACK: " + (loss.spec.isEmpty() ? "không" : loss.spec));
            System.out.println("[nhận] " + SawCommon.maxRtoNote());
            r = sawReceive(s, out, loss);
        }
        System.out.println("[nhận] Hoàn tất: " + r[0] + " byte, " + r[1] + " datagram trùng; SHA-256:");
        System.out.println("[nhận]   " + SawCommon.sha256File(path));
    }
}
