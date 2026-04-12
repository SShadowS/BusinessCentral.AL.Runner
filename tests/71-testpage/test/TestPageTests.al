codeunit 56700 "TP TestPage Tests"
{
    Subtype = Test;

    var
        Assert: Codeunit Assert;

    [Test]
    procedure TestPageOpenEditAndClose()
    var
        TP: TestPage "TP Test Card";
    begin
        // [GIVEN] A TestPage variable
        // [WHEN] OpenEdit and Close are called
        TP.OpenEdit();
        TP.Close();
        // [THEN] Execution completes without error
        Assert.IsTrue(true, 'TestPage OpenEdit/Close must compile and no-op');
    end;

    [Test]
    procedure TestPageOpenViewAndClose()
    var
        TP: TestPage "TP Test Card";
    begin
        TP.OpenView();
        TP.Close();
        Assert.IsTrue(true, 'TestPage OpenView/Close must compile and no-op');
    end;

    [Test]
    procedure TestPageOpenNewAndClose()
    var
        TP: TestPage "TP Test Card";
    begin
        TP.OpenNew();
        TP.Close();
        Assert.IsTrue(true, 'TestPage OpenNew/Close must compile and no-op');
    end;

    [Test]
    procedure TestPageTrap()
    var
        TP: TestPage "TP Test Card";
    begin
        // [GIVEN] A TestPage variable
        // [WHEN] Trap is called (marks page as expecting modal open)
        TP.Trap();
        // [THEN] No crash
        Assert.IsTrue(true, 'TestPage Trap must compile and no-op');
    end;

    [Test]
    procedure TestPageSetValueAndRead()
    var
        TP: TestPage "TP Test Card";
    begin
        // [GIVEN] A TestPage opened for editing
        TP.OpenNew();
        // [WHEN] Setting field values
        TP.NameField.SetValue('Hello');
        TP.AmountField.SetValue(42);
        // [THEN] Values can be read back
        Assert.AreEqual('Hello', TP.NameField.Value, 'Name field should be Hello');
        Assert.AreEqual('42', TP.AmountField.Value, 'Amount field should be 42');
        TP.Close();
    end;

    [Test]
    procedure TestPageSetValueNegative()
    var
        TP: TestPage "TP Test Card";
    begin
        // [GIVEN] A TestPage opened for editing
        TP.OpenNew();
        TP.NameField.SetValue('Hello');
        // [THEN] The value should not be something else
        Assert.AreNotEqual('World', TP.NameField.Value, 'Name field should not be World');
        TP.Close();
    end;

    [Test]
    procedure TestPageOkInvoke()
    var
        TP: TestPage "TP Test Card";
    begin
        // [GIVEN] A TestPage opened for editing with values set
        TP.OpenNew();
        TP.NameField.SetValue('Test');
        // [WHEN] OK action is invoked
        TP.OK().Invoke();
        // [THEN] No crash (the page closes successfully)
        Assert.IsTrue(true, 'OK().Invoke() must compile and no-op');
    end;

    [Test]
    [HandlerFunctions('ConfirmYesHandler')]
    procedure ConfirmHandlerAnswersYes()
    var
        Logic: Codeunit "TP Confirm Logic";
    begin
        // [GIVEN] A ConfirmHandler that replies true
        // [WHEN] Code calls Confirm()
        // [THEN] The handler intercepts and returns true
        Assert.IsTrue(Logic.DoSomethingWithConfirm(), 'Confirm handler should reply true');
    end;

    [ConfirmHandler]
    procedure ConfirmYesHandler(Question: Text; var Reply: Boolean)
    begin
        Reply := true;
    end;

    [Test]
    [HandlerFunctions('ConfirmNoHandler')]
    procedure ConfirmHandlerAnswersNo()
    var
        Logic: Codeunit "TP Confirm Logic";
    begin
        // [GIVEN] A ConfirmHandler that replies false
        // [WHEN] Code calls Confirm()
        // [THEN] The handler intercepts and returns false
        Assert.IsFalse(Logic.DoSomethingWithConfirm(), 'Confirm handler should reply false');
    end;

    [ConfirmHandler]
    procedure ConfirmNoHandler(Question: Text; var Reply: Boolean)
    begin
        Reply := false;
    end;

    [Test]
    [HandlerFunctions('MessageCaptureHandler')]
    procedure MessageHandlerCaptures()
    var
        Logic: Codeunit "TP Confirm Logic";
    begin
        // [GIVEN] A MessageHandler registered
        // [WHEN] Code calls Message()
        Logic.ShowMessage();
        // [THEN] No crash — the handler intercepts the message
        Assert.IsTrue(true, 'MessageHandler should intercept Message call');
    end;

    [MessageHandler]
    procedure MessageCaptureHandler(Msg: Text)
    begin
        // Just capture — no assertion needed here
    end;
}
