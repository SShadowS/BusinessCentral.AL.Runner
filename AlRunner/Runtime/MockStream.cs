namespace AlRunner.Runtime;

using System.Text;
using Microsoft.Dynamics.Nav.Runtime;
using Microsoft.Dynamics.Nav.Types;

/// <summary>
/// Static replacement for ALStream.
/// Routes ALReadText/ALWriteText calls to MockInStream/MockOutStream.
/// </summary>
public static class MockStream
{
    /// <summary>
    /// Default length constant matching ALStream.DefaultLength.
    /// A value of -1 means "read/write all available data".
    /// </summary>
    public static readonly int DefaultLength = -1;

    /// <summary>
    /// ALReadText — reads text from the stream into the ByRef NavText parameter.
    /// Matches: ALStream.ALReadText(INavStreamReader, DataError, ByRef&lt;NavText&gt;, int)
    /// </summary>
    public static int ALReadText(MockInStream reader, DataError dataError, ByRef<NavText> passByRef, int length = -1)
    {
        int maxLen = length < 0 ? int.MaxValue : length;
        string text = "";
        int bytesRead = reader.ReadText(ref text, maxLen);
        passByRef.Value = NavText.Create(text);
        return bytesRead;
    }

    /// <summary>
    /// ALReadText — reads text directly into a ref NavText parameter.
    /// Matches: ALStream.ALReadText(INavStreamReader, DataError, ref NavText, int)
    /// </summary>
    public static int ALReadText(MockInStream reader, DataError dataError, ref NavText value, int length = -1)
    {
        int maxLen = length < 0 ? int.MaxValue : length;
        string text = "";
        int bytesRead = reader.ReadText(ref text, maxLen);
        value = NavText.Create(text);
        return bytesRead;
    }

    /// <summary>
    /// ALWriteText — writes text to the stream.
    /// Matches: ALStream.ALWriteText(INavStreamWriter, DataError, string/NavText, int)
    /// </summary>
    public static int ALWriteText(MockOutStream writer, DataError dataError, NavText value, int length = -1)
    {
        string text = value.ToString();
        if (length >= 0 && length < text.Length)
            text = text.Substring(0, length);
        writer.WriteText(text);
        return Encoding.UTF8.GetByteCount(text);
    }

    /// <summary>
    /// ALWriteText — writes a string to the stream.
    /// </summary>
    public static int ALWriteText(MockOutStream writer, DataError dataError, string value, int length = -1)
    {
        string text = value;
        if (length >= 0 && length < text.Length)
            text = text.Substring(0, length);
        writer.WriteText(text);
        return Encoding.UTF8.GetByteCount(text);
    }

    /// <summary>
    /// ALWriteText — no-arg overload (writes empty line / newline).
    /// Matches: ALStream.ALWriteText(INavStreamWriter, DataError)
    /// </summary>
    public static int ALWriteText(MockOutStream writer, DataError dataError)
    {
        writer.WriteText("");
        return 0;
    }

    /// <summary>
    /// ALWrite — writes a NavText value to the stream as bytes.
    /// </summary>
    public static int ALWrite(MockOutStream writer, DataError dataError, NavText value, int length = -1)
    {
        return ALWriteText(writer, dataError, value, length);
    }

    /// <summary>
    /// ALWrite — writes a string value to the stream as bytes.
    /// </summary>
    public static int ALWrite(MockOutStream writer, DataError dataError, string value, int length = -1)
    {
        return ALWriteText(writer, dataError, value, length);
    }

    /// <summary>
    /// ALRead — reads from stream into ByRef NavText.
    /// </summary>
    public static int ALRead(MockInStream reader, DataError dataError, ByRef<NavText> passByRef, int length = -1)
    {
        return ALReadText(reader, dataError, passByRef, length);
    }
}
