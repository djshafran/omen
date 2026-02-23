from behave import given, when, then, parsers


@given("user has an account")
def user_has_account(context):  # noqa: ARG001
    pass


@when("user submits valid credentials")
def user_submits_credentials(context):  # noqa: ARG001
    pass


@then("dashboard is visible")
def dashboard_is_visible(context):  # noqa: ARG001
    pass


@given(parsers.parse("product \"{product}\" exists"))
def product_exists(context, product):  # noqa: ARG001
    pass


@when(parsers.parse("user adds \"{product}\" to cart"))
def add_to_cart(context, product):  # noqa: ARG001
    pass


@then(parsers.parse("cart contains \"{product}\""))
def cart_contains(context, product):  # noqa: ARG001
    pass
