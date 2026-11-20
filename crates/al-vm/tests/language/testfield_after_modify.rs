//! `Record.TestField` reads the record's own buffer, also after a Modify and inside OnModify: the error names the field,
//! the table and the key the buffer holds at the call (text: record/TestRecordRuntimeTextsAndViews.al:232-268
//! Record_TestField_MismatchTexts), and a passing TestField changes nothing.

use al_builtins::value::Value;

const SRC: &str = r#"
table 50100 TfmRow
{
    fields
    {
        field(1; Id; Code[10]) { }
        field(2; Name; Text[30]) { }
        field(3; Qty; Integer) { }
        field(4; Note; Text[30]) { }
    }
    keys { key(PK; Id) { Clustered = true; } }
    trigger OnModify()
    begin
        Rec.TestField(Qty, 2);
        // xRec mirrors Rec when code drives the Modify (record/TestXRecContracts.al:69-87).
        xRec.TestField(Qty, 2);
        if Rec.Name = 'fail-rec' then
            Rec.TestField(Qty, 9);
        if Rec.Name = 'fail-xrec' then
            xRec.TestField(Note);
    end;
}
codeunit 50100 C
{
    local procedure Seed(var R: Record TfmRow)
    begin
        R.Id := 'K1';
        R.Name := 'first';
        R.Qty := 1;
        R.Insert();
        R.Get('K1');
    end;

    procedure AfterModify(): Text
    var R: Record TfmRow; S: Record TfmRow; T: Text;
    begin
        Seed(R);
        R.Qty := 2;
        R.Modify(true);
        R.TestField(Qty, 2);
        R.TestField(Name, 'first');
        // The stored row after the Modify (an asserterror below rolls it back).
        S.Get('K1');
        S.TestField(Qty, 2);
        T := Format(S.Qty) + '|' + R.Name;
        asserterror R.TestField(Note);
        T += '|' + GetLastErrorText();
        asserterror R.TestField(Qty, 3);
        exit(T + '|' + GetLastErrorText());
    end;

    procedure FailRec(): Text
    begin
        exit(InOnModify('fail-rec'));
    end;

    procedure FailXRec(): Text
    begin
        exit(InOnModify('fail-xrec'));
    end;

    local procedure InOnModify(Which: Text): Text
    var R: Record TfmRow;
    begin
        Seed(R);
        R.Qty := 2;
        R.Name := Which;
        asserterror R.Modify(true);
        exit(GetLastErrorText());
    end;
}
"#;

fn run(routine: &str) -> String {
    match al_testkit::run(SRC, routine) {
        Ok(Value::Text(s)) => s.to_string(),
        other => panic!("{other:?}"),
    }
}

#[test]
fn testfield_after_a_modify_names_the_modified_buffer() {
    assert_eq!(
        run("AfterModify"),
        "2|first\
         |Note must have a value in TfmRow: Id=K1. It cannot be zero or empty.\
         |Qty must be equal to '3'  in TfmRow: Id=K1. Current value is '2'."
    );
}

#[test]
fn testfield_in_on_modify_reads_rec_and_xrec() {
    assert_eq!(
        run("FailRec"),
        "Qty must be equal to '9'  in TfmRow: Id=K1. Current value is '2'."
    );
    assert_eq!(
        run("FailXRec"),
        "Note must have a value in TfmRow: Id=K1. It cannot be zero or empty."
    );
}
