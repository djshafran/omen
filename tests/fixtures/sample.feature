Feature: Checkout checkout
  Scenario: Successful login
    Given user has an account
    When user submits valid credentials
    Then dashboard is visible

  Scenario Outline: Add product to cart
    Given product "<product>" exists
    When user adds "<product>" to cart
    Then cart contains "<product>"

    Examples:
      | product |
      | Widget  |
