import java.util.ArrayList;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;

/**
 * JSON tối giản (JDK không có thư viện JSON chuẩn): JSON_DECODE trả về Map/List/String/Long/Double/Boolean/null,
 * JSON_ENCODE ghi dạng gọn, giữ nguyên chữ có dấu (giống bản Python, C#). Lỗi cú pháp ném IllegalArgumentException.
 */
final class Json {
    private final String s;
    private int pos;

    private Json(String s) { this.s = s; }

    static Object parse(String text) {
        Json p = new Json(text);
        Object v = p.value();
        p.ws();
        if (p.pos != text.length()) throw p.error();
        return v;
    }

    private IllegalArgumentException error() { return new IllegalArgumentException("JSON sai tại vị trí " + pos); }

    private void ws() { while (pos < s.length() && " \t\r\n".indexOf(s.charAt(pos)) >= 0) pos++; }

    private char peek() { ws(); if (pos >= s.length()) throw error(); return s.charAt(pos); }

    private void expect(char c) { if (peek() != c) throw error(); pos++; }

    private Object value() {
        char c = peek();
        if (c == '{') {
            Map<String, Object> m = new LinkedHashMap<>();
            pos++;
            if (peek() == '}') { pos++; return m; }
            do { String k = string(); expect(':'); m.put(k, value()); } while (comma());
            expect('}');
            return m;
        }
        if (c == '[') {
            List<Object> a = new ArrayList<>();
            pos++;
            if (peek() == ']') { pos++; return a; }
            do { a.add(value()); } while (comma());
            expect(']');
            return a;
        }
        if (c == '"') return string();
        for (String w : new String[] {"true", "false", "null"})
            if (s.startsWith(w, pos)) { pos += w.length(); return w.equals("null") ? null : w.equals("true"); }
        int start = pos;
        while (pos < s.length() && "+-0123456789.eE".indexOf(s.charAt(pos)) >= 0) pos++;
        String num = s.substring(start, pos);
        try {
            return num.matches("-?\\d+") ? (Object) Long.parseLong(num) : (Object) Double.parseDouble(num);
        } catch (NumberFormatException e) { pos = start; throw error(); }
    }

    private boolean comma() { if (peek() == ',') { pos++; return true; } return false; }

    private String string() {
        expect('"');
        StringBuilder b = new StringBuilder();
        while (true) {
            if (pos >= s.length()) throw error();
            char c = s.charAt(pos++);
            if (c == '"') return b.toString();
            if (c != '\\') { b.append(c); continue; }
            if (pos >= s.length()) throw error();
            char e = s.charAt(pos++);
            switch (e) {
                case 'n' -> b.append('\n'); case 't' -> b.append('\t'); case 'r' -> b.append('\r');
                case 'b' -> b.append('\b'); case 'f' -> b.append('\f');
                case 'u' -> {
                    if (pos + 4 > s.length()) throw error();
                    try { b.append((char) Integer.parseInt(s.substring(pos, pos + 4), 16)); }
                    catch (NumberFormatException x) { throw error(); }
                    pos += 4;
                }
                default -> b.append(e);              // \" \\ \/
            }
        }
    }

    static String stringify(Object o) {
        if (o == null) return "null";
        if (o instanceof String str) {
            StringBuilder b = new StringBuilder("\"");
            for (char c : str.toCharArray()) {
                if (c == '"' || c == '\\') b.append('\\').append(c);
                else if (c == '\n') b.append("\\n");
                else if (c < 0x20) b.append(String.format("\\u%04x", (int) c));
                else b.append(c);
            }
            return b.append('"').toString();
        }
        if (o instanceof Map<?, ?> m) {
            StringBuilder b = new StringBuilder("{");
            for (Map.Entry<?, ?> e : m.entrySet()) {
                if (b.length() > 1) b.append(',');
                b.append(stringify(String.valueOf(e.getKey()))).append(':').append(stringify(e.getValue()));
            }
            return b.append('}').toString();
        }
        if (o instanceof List<?> a) {
            StringBuilder b = new StringBuilder("[");
            for (Object x : a) { if (b.length() > 1) b.append(','); b.append(stringify(x)); }
            return b.append(']').toString();
        }
        return String.valueOf(o);                    // Number, Boolean
    }
}
