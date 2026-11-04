import java.io.ByteArrayOutputStream;
import java.io.IOException;
import java.io.InputStream;
import java.nio.charset.StandardCharsets;
import java.util.Arrays;
import java.util.LinkedHashMap;
import java.util.Map;

/** Mã giả 3.5 – STRUCT Reader { sock, pending }: giữ phần byte nhận dư giữa các lần đọc. */
final class Reader {
    private final InputStream in;
    private byte[] pending = new byte[0];

    Reader(InputStream in) { this.in = in; }

    /** Mã giả 3.5 – READ_LINE: trả về dòng (bỏ CRLF) hoặc null khi đối tác đã đóng. */
    String readLine(int maxLen) throws IOException {
        while (true) {
            int i = indexOf(pending, (byte) '\n');
            if (i > maxLen) throw new IOException("Dòng quá dài");   // Mã giả 3.5 dòng 5: có LF nhưng dòng quá dài
            if (i >= 0) {
                int len = (i > 0 && pending[i - 1] == '\r') ? i - 1 : i;     // STRIP_CR
                String line = new String(pending, 0, len, StandardCharsets.UTF_8);
                pending = Arrays.copyOfRange(pending, i + 1, pending.length);
                return line;
            }
            if (pending.length > maxLen) throw new IOException("Dòng quá dài");
            byte[] buf = new byte[4096];
            int n = in.read(buf);
            if (n == -1) return null;
            byte[] merged = Arrays.copyOf(pending, pending.length + n);
            System.arraycopy(buf, 0, merged, pending.length, n);
            pending = merged;
        }
    }

    /** READ_EXACT: lấy phần dư pending trước, thiếu thì RECV_EXACT (Mã giả 3.4). */
    byte[] readExact(int n) throws IOException {
        byte[] data = new byte[n];
        int have = Math.min(n, pending.length);
        System.arraycopy(pending, 0, data, 0, have);
        pending = Arrays.copyOfRange(pending, have, pending.length);
        while (have < n) {
            int k = in.read(data, have, n - have);
            if (k == -1) throw new IOException("Kết nối đóng khi mới nhận " + have + "/" + n + " byte");
            have += k;
        }
        return data;
    }

    /** READ_UNTIL_CLOSE: đọc đến khi server đóng kết nối. */
    byte[] readUntilClose() throws IOException {
        ByteArrayOutputStream out = new ByteArrayOutputStream();
        out.write(pending);
        pending = new byte[0];
        in.transferTo(out);
        return out.toByteArray();
    }

    /** Mã giả 7.1 – READ_HEADERS: đọc đến dòng trống, tên trường đổi về chữ thường. */
    Map<String, String> readHeaders() throws IOException {
        Map<String, String> hdr = new LinkedHashMap<>();
        while (true) {
            String line = readLine(8192);
            if (line == null)                                            // Mã giả 7.1 dòng 29: đóng giữa chừng
                throw new java.io.EOFException("Kết nối đóng khi chưa đọc hết phần tiêu đề");   // ConnectionClosed
            if (line.isEmpty()) return hdr;                              // dòng 30: dòng trống – hết
            int i = line.indexOf(':');
            String k = i >= 0 ? line.substring(0, i) : line, v = i >= 0 ? line.substring(i + 1) : "";
            hdr.put(k.trim().toLowerCase(), v.trim());
        }
    }

    private static int indexOf(byte[] a, byte b) {
        for (int i = 0; i < a.length; i++) if (a[i] == b) return i;
        return -1;
    }
}
