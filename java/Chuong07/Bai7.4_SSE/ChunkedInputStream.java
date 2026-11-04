import java.io.IOException;
import java.io.InputStream;

/** Lớp giải mã khối (READ_CHUNKED của Mã giả 7.1 viết dạng luồng): read() trả dữ liệu thân thuần. */
final class ChunkedInputStream extends InputStream {
    private final Reader r;
    private int left;
    private boolean done;

    ChunkedInputStream(Reader r) { this.r = r; }

    @Override
    public int read(byte[] buf, int off, int len) throws IOException {
        if (done) return -1;
        if (left == 0) {
            String sizeLine = r.readLine(1024);
            int size = sizeLine == null ? 0 : Integer.parseInt(sizeLine.split(";")[0].trim(), 16);
            if (size == 0) { r.readHeaders(); done = true; return -1; }  // khối 0: hết thân, bỏ trailer
            left = size;
        }
        byte[] data = r.readExact(Math.min(len, left));
        System.arraycopy(data, 0, buf, off, data.length);
        left -= data.length;
        if (left == 0) r.readLine(2);                                   // CRLF sau mỗi khối
        return data.length;
    }

    @Override
    public int read() throws IOException {
        byte[] one = new byte[1];
        return read(one, 0, 1) == -1 ? -1 : one[0] & 0xFF;
    }
}
