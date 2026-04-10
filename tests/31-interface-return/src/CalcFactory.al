codeunit 50101 "Calc Factory"
{
    procedure GetCalculator(): Interface "ICalc"
    var
        Adder: Codeunit "Calc Adder";
    begin
        exit(Adder);
    end;
}
