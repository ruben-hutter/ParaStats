from time import sleep
from selenium import webdriver
from selenium.webdriver.common.keys import Keys
from selenium.webdriver.common.by import By
from selenium.webdriver.support.ui import WebDriverWait
from selenium.webdriver.support import expected_conditions as EC
import zipfile
import os
from datetime import date

driver = webdriver.Firefox()

# login data
print("===== Enter login data =====")
username = input("Username: ")
password = input("Password: ")
print("===== Getting flights =====")

# variables
url = "https://www.xcontest.org/world/en/"
download_path = os.path.join(os.environ['HOME'], "Downloads/")
file_name = f"igc.{username}.world{str(date.today().year)}.zip"
file_path = download_path + file_name

# open webpage
driver.get(url)
# wait to load page
driver.implicitly_wait(3)
# enter username & password for login
login_username = driver.find_element(By.ID, "login-username")
login_username.send_keys(username)
login_password = driver.find_element(By.ID, "login-password")
login_password.send_keys(password)
login_password.send_keys(Keys.ENTER)
# go to my_flights
my_flights = driver.find_element(By.LINK_TEXT, "My flights")
my_flights.click()
# download all flights
print("===== Downloading... =====")
download_flights = driver.find_element(By.LINK_TEXT, "IGC")
driver.execute_script("arguments[0].click();", download_flights)
# wait until download completed
dl_completed = False
while not dl_completed:
    sleep(1)
    # check if file exists
    if os.path.exists(file_path) and os.path.isfile(file_path):
        dl_completed = True
print("===== Finished download =====")
# extract flights
print("===== Extracting... =====")
with zipfile.ZipFile(file_path, 'r') as zip_ref:
    zip_ref.extractall(f"{download_path}/{username}_flights")

driver.quit()
print("===== Finished =====")