codeunit 50100 "Calc Adder" implements "ICalc"
{
    procedure Calculate(A: Decimal; B: Decimal): Decimal
    begin
        exit(A + B);
    end;
}
