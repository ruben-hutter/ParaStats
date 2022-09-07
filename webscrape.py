import os
import time
import zipfile
from datetime import date
from time import sleep

from selenium import webdriver
from selenium.webdriver.common.by import By
from selenium.webdriver.common.keys import Keys

# max time for download
TIMEOUT = 20
DOWNLOAD_PATH = os.path.join(os.environ['HOME'], "Downloads/")
URL = "https://www.xcontest.org/world/en/"
# start webdriver
DRIVER = webdriver.Firefox()
initial_number_of_files = 0


def get_user_data():
    print("===== Enter login data =====")
    username = input("Username: ")
    password = input("Password: ")
    return username, password


def login(username, password):
    login_username = DRIVER.find_element(By.ID, "login-username")
    login_username.send_keys(username)
    login_password = DRIVER.find_element(By.ID, "login-password")
    login_password.send_keys(password)
    login_password.send_keys(Keys.ENTER)


def download_flights(file_path):
    global initial_number_of_files
    print("===== Downloading... =====")
    initial_number_of_files = check_number_of_files(DOWNLOAD_PATH)
    download_igc = DRIVER.find_element(By.LINK_TEXT, "IGC")
    DRIVER.execute_script("arguments[0].click();", download_igc)
    # wait until download completed
    # 1) firefox creates the end file and a tmp file for the download
    # 2) count number of files before download
    # 3) check until only 1 new file instead of 2 -> download finished
    dl_completed = False
    t_start = time.time()
    t_elapsed = 0
    while not dl_completed and t_elapsed < TIMEOUT:
        # check if file exists
        if os.path.isfile(file_path) and check_number_of_files(DOWNLOAD_PATH) == initial_number_of_files + 1:
            print("===== Finished download =====")
            return True, True
        sleep(1)
        t_elapsed = time.time() - t_start
    # check if download started
    if os.path.isfile(file_path):
        return False, True
    return False, False


def check_number_of_files(path):
    directory = os.listdir(path)
    number_of_files = len(directory)
    return number_of_files


def extract_flights(file_path, username):
    print("===== Extracting... =====")
    with zipfile.ZipFile(file_path, 'r') as zip_ref:
        zip_ref.extractall(f"{DOWNLOAD_PATH}/{username}_flights")
    print("===== Finished =====")


def remove_temp_files():
    for file in os.listdir(DOWNLOAD_PATH):
        if os.path.isfile(DOWNLOAD_PATH + file) and file.endswith(".zip.part"):
            os.remove(DOWNLOAD_PATH + file)


def wait_download(file_path):
    # check if download already finished
    dl_completed = False
    t_start = time.time()
    t_elapsed = 0
    while not dl_completed and t_elapsed < TIMEOUT:
        # check if file exists
        if os.path.isfile(file_path) and check_number_of_files(DOWNLOAD_PATH) == initial_number_of_files + 1:
            print("===== Finished download =====")
            return True
        sleep(1)
        t_elapsed = time.time() - t_start


def main():
    # get user's xcontest credentials
    username, password = get_user_data()
    file_name = f"igc.{username}.world{str(date.today().year)}.zip"
    file_path = DOWNLOAD_PATH + file_name
    # open webpage
    DRIVER.get(URL)
    # wait to load page
    DRIVER.implicitly_wait(3)
    # login with credentials
    login(username, password)
    # go to my_flights
    my_flights = DRIVER.find_element(By.LINK_TEXT, "My flights")
    my_flights.click()
    # download all flights
    download_ok, download_started = download_flights(file_path)
    while not download_ok:
        # check if flights downloaded correctly
        if not download_started:
            print("===== Download failed! =====")
            choice = input("[R]etry | [C]ancel: ").lower()
            if choice == "r":
                download_ok, download_started = download_flights(file_path)
            else:
                break
        else:
            print("===== Download timeout! =====")
            choice = input("[W]ait | [C]ancel: ").lower()
            if choice == "w":
                download_ok = wait_download(file_path)
            else:
                break
    if download_ok:
        # extract flights
        extract_flights(file_path, username)
    # remove possible temp files
    remove_temp_files()
    DRIVER.quit()


if __name__ == "__main__":
    main()
