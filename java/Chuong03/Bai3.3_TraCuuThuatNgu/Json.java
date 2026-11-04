import java.util.ArrayList;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;

/**
 * JSON tối giản theo RFC 8259 (JSON_ENCODE, JSON_DECODE của Mã giả 3.6) – Java SE không có thư viện JSON chuẩn.
 * Ánh xạ kiểu: đối tượng ↔ Map (giữ thứ tự khóa), mảng ↔ List, chuỗi ↔ String,
 * số ↔ Long hoặc Double, true/false ↔ Boolean, null ↔ null.
 */
final class Json {
    private Json() {}

    /** Lỗi cú pháp JSON hoặc nội dung không phải UTF-8 hợp lệ. */
    static final class JsonError extends IllegalArgumentException {
        JsonError(String m) { super(m); }
    }

    /** Tạo đối tượng JSON giữ thứ tự khóa: obj("status", 200, "text", "…"). */
    static Map<String, Object> obj(Object... kv) {
        Map<String, Object> m = new LinkedHashMap<>();
        for (int i = 0; i + 1 < kv.length; i += 2) m.put((String) kv[i], kv[i + 1]);
        return m;
    }

    static String stringify(Object v) {
        StringBuilder sb = new StringBuilder();
        write(sb, v);
        return sb.toString();
    }

    private static void write(StringBuilder sb, Object v) {
        if (v == null) sb.append("null");
        else if (v instanceof String s) writeString(sb, s);
        else if (v instanceof Number || v instanceof Boolean) sb.append(v);
        else if (v instanceof Map<?, ?> m) {
            sb.append('{');
            boolean first = true;
            for (Map.Entry<?, ?> e : m.entrySet()) {
                if (!first) sb.append(',');
                first = false;
                writeString(sb, String.valueOf(e.getKey()));
                sb.append(':');
                write(sb, e.getValue());
            }
            sb.append('}');
        } else if (v instanceof List<?> l) {
            sb.append('[');
            for (int i = 0; i < l.size(); i++) {
                if (i > 0) sb.append(',');
                write(sb, l.get(i));
            }
            sb.append(']');
        } else throw new JsonError("Kiểu không hỗ trợ: " + v.getClass().getSimpleName());
    }

    /** Chỉ thoát dấu nháy, gạch chéo ngược và ký tự điều khiển; giữ nguyên chữ tiếng Việt. */
    private static void writeString(StringBuilder sb, String s) {
        sb.append('"');
        for (int i = 0; i < s.length(); i++) {
            char c = s.charAt(i);
            switch (c) {
                case '"' -> sb.append("\\\"");
                case '\\' -> sb.append("\\\\");
                case '\n' -> sb.append("\\n");
                case '\r' -> sb.append("\\r");
                case '\t' -> sb.append("\\t");
                default -> {
                    if (c < 0x20) sb.append(String.format("\\u%04x", (int) c));
                    else sb.append(c);
                }
            }
        }
        sb.append('"');
    }

    static Object parse(String text) {
        Parser p = new Parser(text);
        p.ws();
        Object v = p.value();
        p.ws();
        if (p.i != text.length()) throw p.err();
        return v;
    }

    /** Bộ phân tích đệ quy xuống (recursive descent) trên chuỗi văn bản JSON. */
    private static final class Parser {
        final String t;
        int i;

        Parser(String t) { this.t = t; }

        JsonError err() { return new JsonError("JSON sai tại vị trí " + i); }

        void ws() { while (i < t.length() && " \t\r\n".indexOf(t.charAt(i)) >= 0) i++; }

        void expect(char c) { if (i >= t.length() || t.charAt(i) != c) throw err(); i++; }

        Object value() {
            if (i >= t.length()) throw err();
            char c = t.charAt(i);
            if (c == '{') return object();
            if (c == '[') return array();
            if (c == '"') return string();
            if (t.startsWith("true", i)) { i += 4; return Boolean.TRUE; }
            if (t.startsWith("false", i)) { i += 5; return Boolean.FALSE; }
            if (t.startsWith("null", i)) { i += 4; return null; }
            return number();
        }

        Map<String, Object> object() {
            Map<String, Object> m = new LinkedHashMap<>();
            expect('{'); ws();
            if (i < t.length() && t.charAt(i) == '}') { i++; return m; }
            while (true) {
                ws(); String k = string(); ws(); expect(':'); ws();
                m.put(k, value()); ws();
                if (i < t.length() && t.charAt(i) == ',') { i++; continue; }
                expect('}');
                return m;
            }
        }

        List<Object> array() {
            List<Object> l = new ArrayList<>();
            expect('['); ws();
            if (i < t.length() && t.charAt(i) == ']') { i++; return l; }
            while (true) {
                ws(); l.add(value()); ws();
                if (i < t.length() && t.charAt(i) == ',') { i++; continue; }
                expect(']');
                return l;
            }
        }

        String string() {
            expect('"');
            StringBuilder sb = new StringBuilder();
            while (true) {
                if (i >= t.length()) throw err();
                char c = t.charAt(i++);
                if (c == '"') return sb.toString();
                if (c < 0x20) throw err();
                if (c != '\\') { sb.append(c); continue; }
                if (i >= t.length()) throw err();
                char e = t.charAt(i++);
                switch (e) {
                    case '"', '\\', '/' -> sb.append(e);
                    case 'b' -> sb.append('\b');
                    case 'f' -> sb.append('\f');
                    case 'n' -> sb.append('\n');
                    case 'r' -> sb.append('\r');
                    case 't' -> sb.append('\t');
                    case 'u' -> {
                        if (i + 4 > t.length()) throw err();
                        try { sb.append((char) Integer.parseInt(t.substring(i, i + 4), 16)); }
                        catch (NumberFormatException x) { throw err(); }
                        i += 4;
                    }
                    default -> throw err();
                }
            }
        }

        Object number() {
            int start = i;
            while (i < t.length() && "+-0123456789.eE".indexOf(t.charAt(i)) >= 0) i++;
            String s = t.substring(start, i);
            if (!s.matches("-?(0|[1-9]\\d*)(\\.\\d+)?([eE][+-]?\\d+)?")) { i = start; throw err(); }
            if (s.matches("-?\\d+")) {
                try { return Long.parseLong(s); } catch (NumberFormatException x) { /* quá lớn: dùng Double */ }
            }
            return Double.parseDouble(s);
        }
    }
}
