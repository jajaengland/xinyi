"""Shared coordinates for the desktop shell (not the legacy windowtest demo)."""
APPS_BUTTON=(56,576)
SHELL_BUTTON=(756,576)
def app_card(index):return (48+(index%2)*228,188+(index//2)*44)
def task_button(index):return (140+index*116,576)
